use crate::BlockExtra;
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::hashes::hex::FromHex;
use bitcoin::{BlockHash, Network};
use log::{info, log, warn, Level};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Reorder {
    receiver: Receiver<Option<BlockExtra>>,
    sender: SyncSender<Option<BlockExtra>>,
    height: u32,
    next: BlockHash,
    blocks: OutOfOrderBlocks,
}

struct OutOfOrderBlocks {
    blocks: HashMap<BlockHash, BlockExtra>,
    follows: HashMap<BlockHash, Vec<BlockHash>>,
    max_reorg: u8,
}

impl OutOfOrderBlocks {
    fn new(max_reorg: u8) -> Self {
        OutOfOrderBlocks {
            blocks: HashMap::default(),
            follows: HashMap::default(),
            max_reorg,
        }
    }

    fn add(&mut self, mut block_extra: BlockExtra) {
        let prev_hash = block_extra.block.header.prev_blockhash;
        self.follows
            .entry(prev_hash)
            .and_modify(|e| e.push(block_extra.block_hash))
            .or_insert(vec![block_extra.block_hash]);

        if let Some(follows) = self.follows.remove(&block_extra.block_hash) {
            for el in follows {
                block_extra.next.push(el);
            }
        }

        if let Some(prev_block) = self.blocks.get_mut(&prev_hash) {
            prev_block.next.push(block_extra.block_hash);
        }

        self.blocks.insert(block_extra.block_hash, block_extra);
    }

    /// check the block identified by `hash` has at least `n` blocks after, to be sure it's not a reorged block
    /// keep track of the followed path with `first_next` that should be initialized with `None` in the first call
    fn exist_and_has_followers(&self, hash: &BlockHash, path: Vec<BlockHash>) -> Option<BlockHash> {
        if let Some(block) = self.blocks.get(hash) {
            for next in block.next.iter() {
                return if path.len() == self.max_reorg as usize {
                    Some(path[0])
                } else {
                    let mut path = path.clone();
                    path.push(*next);
                    self.exist_and_has_followers(next, path)
                };
            }
        }
        None
    }

    fn remove(&mut self, hash: &BlockHash) -> Option<BlockExtra> {
        if let Some(next) = self.exist_and_has_followers(hash, vec![]) {
            let mut value = self.blocks.remove(hash).unwrap();
            if value.next.len() > 1 {
                warn!(
                    "at {} fork to {:?} took {}",
                    value.block_hash, value.next, next
                );
            }
            value.next = vec![next];
            Some(value)
        } else {
            None
        }
    }
}

impl Reorder {
    pub fn new(
        network: Network,
        max_reorg: u8,
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Reorder {
        Reorder {
            sender,
            receiver,
            height: 0,
            next: genesis_block(network).block_hash(),
            blocks: OutOfOrderBlocks::new(max_reorg),
        }
    }

    fn send(&mut self, mut block_extra: BlockExtra) {
        self.next = block_extra.next[0];
        block_extra.height = self.height;
        self.blocks.follows.remove(&block_extra.block_hash);
        self.sender.send(Some(block_extra)).unwrap();
        self.height += 1;
    }

    pub fn start(&mut self) {
        let mut busy_time = 0u128;
        let mut count = 0u32;
        loop {
            let received = self.receiver.recv().expect("cannot receive blob");
            let now = Instant::now();
            match received {
                Some(block_extra) => {
                    let level = if count % 10_000 == 0 {
                        Level::Info
                    } else {
                        Level::Debug
                    };

                    log!(
                        level,
                        "reorder receive:{} size:{} follows:{} height:{} next:{}",
                        block_extra.block_hash,
                        self.blocks.blocks.len(),
                        self.blocks.follows.len(),
                        self.height,
                        self.next
                    );

                    count += 1;

                    if self.blocks.blocks.len() > 2_000
                    {
                        for block in self.blocks.blocks.values() {
                            println!("{} {:?}", block.block_hash, block.next);
                        }
                        panic!();
                    }
                    self.blocks.add(block_extra);
                    while let Some(block_to_send) = self.blocks.remove(&self.next) {
                        self.send(block_to_send);
                    }
                }
                None => break,
            }
            busy_time += now.elapsed().as_nanos();
        }
        info!(
            "ending reorder next:{} #elements:{} #follows:{}",
            self.next,
            self.blocks.blocks.len(),
            self.blocks.follows.len()
        );
        info!(
            "ending reorder, busy time(*): {}s",
            busy_time / 1_000_000_000
        );
        self.sender.send(None).expect("reorder cannot send none");
    }
}

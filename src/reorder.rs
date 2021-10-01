use crate::{periodic_log_level, FsBlock};
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::{BlockHash, Network};
use log::{info, log, warn};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Reorder {
    receiver: Receiver<Option<Vec<FsBlock>>>,
    sender: SyncSender<Option<FsBlock>>,
    next: BlockHash,
    blocks: OutOfOrderBlocks,
}

struct OutOfOrderBlocks {
    blocks: HashMap<BlockHash, FsBlock>,
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

    fn add(&mut self, mut raw_block: FsBlock) {
        let prev_hash = raw_block.prev;
        self.follows
            .entry(prev_hash)
            .and_modify(|e| e.push(raw_block.hash))
            .or_insert_with(|| vec![raw_block.hash]);

        if let Some(follows) = self.follows.remove(&raw_block.hash) {
            for el in follows {
                raw_block.next.push(el);
            }
        }

        if let Some(prev_block) = self.blocks.get_mut(&prev_hash) {
            prev_block.next.push(raw_block.hash);
        }

        self.blocks.insert(raw_block.hash, raw_block);
    }

    /// check the block identified by `hash` has at least `self.max_reorgs` blocks after, to be sure it's not a reorged block
    /// keep track of the followed `path` that should be initialized with empty vec in the first call
    fn exist_and_has_followers(&self, hash: &BlockHash, path: Vec<BlockHash>) -> Option<BlockHash> {
        if path.len() == self.max_reorg as usize {
            return Some(path[0]);
        }
        if let Some(block) = self.blocks.get(hash) {
            for next in block.next.iter() {
                let mut path = path.clone();
                path.push(*next);
                if let Some(hash) = self.exist_and_has_followers(next, path) {
                    return Some(hash);
                }
            }
        }
        None
    }

    fn remove(&mut self, hash: &BlockHash) -> Option<FsBlock> {
        if let Some(next) = self.exist_and_has_followers(hash, vec![]) {
            let mut value = self.blocks.remove(hash).unwrap();
            if value.next.len() > 1 {
                warn!("at {} fork to {:?} took {}", value.hash, value.next, next);
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
        receiver: Receiver<Option<Vec<FsBlock>>>,
        sender: SyncSender<Option<FsBlock>>,
    ) -> Reorder {
        Reorder {
            sender,
            receiver,
            next: genesis_block(network).block_hash(),
            blocks: OutOfOrderBlocks::new(max_reorg),
        }
    }

    fn send(&mut self, fs_block: FsBlock) {
        self.next = fs_block.next[0];
        self.blocks.follows.remove(&fs_block.hash);
        self.blocks.blocks.remove(&fs_block.prev);
        self.sender.send(Some(fs_block)).unwrap();
    }

    pub fn start(&mut self) {
        let mut busy_time = 0u128;
        let mut count = 0u32;
        let mut now;
        loop {
            let received = self.receiver.recv().expect("cannot receive blob");
            now = Instant::now();
            match received {
                Some(raw_blocks) => {
                    for raw_block in raw_blocks {
                        log!(
                            periodic_log_level(count),
                            "reorder receive:{} size:{} follows:{} next:{}",
                            raw_block.hash,
                            self.blocks.blocks.len(),
                            self.blocks.follows.len(),
                            self.next
                        );

                        count += 1;

                        if self.blocks.blocks.len() > 10_000 {
                            for block in self.blocks.blocks.values() {
                                println!("{} {:?}", block.hash, block.next);
                            }
                            println!("next: {}", self.next);
                            panic!();
                        }
                        self.blocks.add(raw_block);
                        while let Some(fs_block) = self.blocks.remove(&self.next) {
                            busy_time += now.elapsed().as_nanos();
                            self.send(fs_block);
                            now = Instant::now();
                        }
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
        info!("ending reorder, busy time: {}s", busy_time / 1_000_000_000);
        self.sender.send(None).expect("reorder cannot send none");
    }
}

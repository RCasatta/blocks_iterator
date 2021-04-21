use crate::BlockExtra;
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::{BlockHash, Network};
use fxhash::FxHashMap;
use log::{debug, info};
use std::collections::hash_map::Iter;
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

struct OutOfOrderBlocks(FxHashMap<BlockHash, BlockExtra>, u8);

impl OutOfOrderBlocks {
    fn new(max_reorg: u8) -> Self {
        OutOfOrderBlocks(FxHashMap::default(), max_reorg)
    }

    fn add(&mut self, mut block_extra: BlockExtra) {
        let prev_hash = block_extra.block.header.prev_blockhash;
        for (key, value) in self.0.iter() {
            if value.block.header.prev_blockhash == block_extra.block_hash {
                block_extra.next.push(*key);
            }
        }
        if let Some(prev_block) = self.0.get_mut(&prev_hash) {
            prev_block.next.push(block_extra.block_hash);
        }

        self.0.insert(block_extra.block_hash, block_extra);
    }

    /// check the block identified by `hash` has at least `n` blocks after, to be sure it's not a reorged block
    /// keep track of the followed path with `first_next` that should be initialized with `None` in the first call
    fn exist_and_has_n_following(
        &self,
        hash: &BlockHash,
        n: u8,
        first_next: &mut Option<BlockHash>,
    ) -> bool {
        if let Some(block) = self.0.get(hash) {
            for next in block.next.iter() {
                return if n == 0 {
                    true
                } else {
                    if first_next.is_none() {
                        *first_next = Some(*next)
                    }
                    self.exist_and_has_n_following(next, n - 1, first_next)
                };
            }
        }
        false
    }

    fn remove(&mut self, hash: &BlockHash) -> Option<BlockExtra> {
        let mut next = None;
        if self.exist_and_has_n_following(hash, self.1, &mut next) {
            let value = self.0.remove(hash).unwrap();
            if value.next.len() > 1 {
                info!("after {} had a fork to {:?}", value.block_hash, value.next);
            }
            Some(value)
        } else {
            None
        }
    }

    fn iter(&self) -> Iter<'_, BlockHash, BlockExtra> {
        self.0.iter()
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
        self.sender
            .send(Some(block_extra))
            .expect("reorder: cannot send block");
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
                    debug!("reorder received {}", block_extra.block_hash);
                    if count % 20_000 == 0 {
                        info!("reorder size: {}", self.blocks.0.len())
                    }
                    count += 1;
                    self.blocks.add(block_extra);
                    while let Some(block_to_send) = self.blocks.remove(&self.next) {
                        self.send(block_to_send);
                    }
                }
                None => break,
            }
            busy_time += now.elapsed().as_nanos();
        }
        for (key, value) in self.blocks.iter() {
            info!(
                "not connected: # {:7} hash {} prev {} next {:?}",
                value.height, key, value.block.header.prev_blockhash, value.next
            );
        }
        self.sender.send(None).expect("reorder cannot send none");
        info!(
            "ending reorder, busy time(*): {}",
            busy_time / 1_000_000_000
        );
    }
}

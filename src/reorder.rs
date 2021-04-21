use crate::BlockExtra;
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::{BlockHash, Network};
use log::{debug, info};
use std::collections::hash_map::Iter;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;

pub struct Reorder {
    receiver: Receiver<Option<BlockExtra>>,
    sender: SyncSender<Option<BlockExtra>>,
    height: u32,
    next: BlockHash,
    blocks: OutOfOrderBlocks,
}

struct OutOfOrderBlocks(HashMap<BlockHash, BlockExtra>);

impl OutOfOrderBlocks {
    fn new() -> Self {
        OutOfOrderBlocks(HashMap::new())
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
        if self.exist_and_has_n_following(hash, 3, &mut next) {
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
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Reorder {
        Reorder {
            sender,
            receiver,
            height: 0,
            next: genesis_block(network).block_hash(),
            blocks: OutOfOrderBlocks::new(),
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
        loop {
            let received = self.receiver.recv().expect("cannot receive blob");
            match received {
                Some(block_extra) => {
                    debug!("reorder received {}", block_extra.block_hash);
                    self.blocks.add(block_extra);
                    while let Some(block_to_send) = self.blocks.remove(&self.next) {
                        self.send(block_to_send);
                    }
                }
                None => break,
            }
        }
        for (key, value) in self.blocks.iter() {
            info!(
                "not connected: hash {} prev {} next {:?}",
                key, value.block.header.prev_blockhash, value.next
            );
        }
        self.sender.send(None).expect("reorder cannot send none");
        info!("ending reorder");
    }
}

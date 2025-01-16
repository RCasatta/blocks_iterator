use crate::{BlockExtra, FsBlock, PeriodCounter, Periodic};
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::{BlockHash, Network};
use log::{info, warn};
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct Reorder {
    join: Option<JoinHandle<()>>,
}

impl Drop for Reorder {
    fn drop(&mut self) {
        if let Some(jh) = self.join.take() {
            jh.join().expect("thread failed");
        }
    }
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
        stop_at_height: Option<u32>,
        early_stop: Arc<AtomicBool>,
        receiver: Receiver<Option<Vec<FsBlock>>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Self {
        let mut next = genesis_block(network).block_hash();
        let mut blocks = OutOfOrderBlocks::new(max_reorg);
        let mut height = 0;
        let mut periodic = Periodic::new(Duration::from_secs(60));
        Self {
            join: Some(std::thread::spawn(move || {
                info!("starting reorder");

                let mut bench = PeriodCounter::new(Duration::from_secs(10));

                let mut busy_time = 0u128;
                let mut now = Instant::now();
                let mut last_height = 0;
                loop {
                    busy_time += now.elapsed().as_nanos();
                    let received = receiver.recv().unwrap_or_default();

                    now = Instant::now();
                    match received {
                        Some(raw_blocks) => {
                            if early_stop.load(Ordering::SeqCst) {
                                break;
                            }
                            'outer: for raw_block in raw_blocks {
                                if periodic.elapsed() {
                                    info!(
                                        "reorder receive:{} size:{} follows:{} next:{}",
                                        raw_block.hash,
                                        blocks.blocks.len(),
                                        blocks.follows.len(),
                                        next
                                    );
                                }

                                // even tough should be 1024 -> https://github.com/bitcoin/bitcoin/search?q=BLOCK_DOWNLOAD_WINDOW
                                // in practice it needs to be greater
                                let max_block_to_reorder = 10_000;
                                if blocks.blocks.len() > max_block_to_reorder {
                                    for block in blocks.blocks.values() {
                                        println!("{} {:?}", block.hash, block.next);
                                    }
                                    println!("next: {}", next);
                                    panic!("Reorder map grow more than {}", max_block_to_reorder);
                                }
                                blocks.add(raw_block);
                                while let Some(block_to_send) = blocks.remove(&next) {
                                    let mut block_extra: BlockExtra =
                                        block_to_send.try_into().unwrap();
                                    busy_time += now.elapsed().as_nanos();
                                    next = block_extra.next[0];
                                    block_extra.height = height;
                                    blocks.follows.remove(&block_extra.block_hash);
                                    let block = block_extra.block();

                                    blocks.blocks.remove(&block.header.prev_blockhash);

                                    bench.count_block(&block_extra);
                                    if let Some(stats) = bench.period_elapsed() {
                                        info!(
                                            "# {:7} {}",
                                            block_extra.height, block_extra.block_hash,
                                        );
                                        info!("{}", stats);
                                    }
                                    sender.send(Some(block_extra)).unwrap();

                                    height += 1;
                                    now = Instant::now();
                                    last_height = height;
                                    if let Some(stop_at_height) = stop_at_height {
                                        if height > stop_at_height {
                                            info!("reached height: {}", stop_at_height);
                                            early_stop.store(true, Ordering::Relaxed);
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }
                        None => break,
                    }
                }
                info!(
                    "ending reorder next:{} #elements:{} #follows:{}",
                    next,
                    blocks.blocks.len(),
                    blocks.follows.len()
                );
                info!(
                    "ending reorder, busy time: {}s, last height: {}",
                    busy_time / 1_000_000_000,
                    last_height
                );
                // if !early_stop.load(Ordering::Relaxed) {
                sender.send(None).expect("reorder cannot send none");
                // }
            })),
        }
    }
}

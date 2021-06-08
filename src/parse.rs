use crate::BlockExtra;
use bitcoin::consensus::{deserialize, Decodable};
use bitcoin::{Block, BlockHash, Network};
use log::{error, info, warn};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Seek, SeekFrom};
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Instant;

pub struct Parse {
    network: Network,
    seen: HashSet<BlockHash>,
    receiver: Receiver<Option<Vec<u8>>>,
    sender: SyncSender<Option<BlockExtra>>,
}

impl Parse {
    pub fn new(
        network: Network,
        receiver: Receiver<Option<Vec<u8>>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Parse {
        Parse {
            network,
            seen: HashSet::new(),
            sender,
            receiver,
        }
    }

    pub fn start(&mut self) {
        let mut total_blocks = 0usize;

        let mut busy_time = 0u128;
        let mut now;
        loop {
            let received = self.receiver.recv().expect("cannot receive blob");
            now = Instant::now();
            match received {
                Some(blob) => {
                    let blocks_vec = parse_blocks(self.network.magic(), blob);

                    total_blocks += blocks_vec.len();
                    info!(
                        "This blob contain {} blocks (total {})",
                        blocks_vec.len(),
                        total_blocks
                    );

                    for block in blocks_vec {
                        if !self.seen.contains(&block.block_hash) {
                            self.seen.insert(block.block_hash);
                            busy_time += now.elapsed().as_nanos();
                            self.sender.send(Some(block)).unwrap();
                            now = Instant::now();
                        } else {
                            warn!("duplicate block {}", block.block_hash);
                        }
                    }
                }
                None => break,
            }
        }
        self.sender.send(None).unwrap();
        info!("ending parser, busy time: {}s", (busy_time / 1_000_000_000));
    }
}

fn parse_blocks(magic: u32, blob: Vec<u8>) -> Vec<BlockExtra> {
    let mut cursor = Cursor::new(&blob);
    let mut blocks = vec![];
    let max_pos = blob.len() as u64;
    while cursor.position() < max_pos {
        match u32::consensus_decode(&mut cursor) {
            Ok(value) => {
                if magic != value {
                    cursor
                        .seek(SeekFrom::Current(-3))
                        .expect("failed to seek back");
                    continue;
                }
            }
            Err(_) => break, // EOF
        };
        let size = u32::consensus_decode(&mut cursor).expect("a");
        let start = cursor.position() as usize;
        cursor
            .seek(SeekFrom::Current(i64::from(size)))
            .expect("failed to seek forward");
        let end = cursor.position() as usize;

        match deserialize::<Block>(&blob[start..end]) {
            Ok(block) => {
                let block_hash = block.block_hash();
                blocks.push(BlockExtra {
                    block,
                    block_hash,
                    size: (end - start) as u32,
                    height: 0,
                    next: vec![],
                    outpoint_values: HashMap::new(),
                    tx_hashes: HashSet::new(),
                })
            }
            Err(e) => error!("error block parsing {:?}", e),
        }
    }
    blocks
}

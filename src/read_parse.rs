use crate::bitcoin::consensus::Decodable;
use crate::bitcoin::{BlockHash, BlockHeader, Network};
use crate::BlockSlice;
use log::{error, info};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::Instant;

pub struct ReadParse {
    blocks_dir: PathBuf,
    network: Network,
    seen: Seen,
    sender: SyncSender<Option<BlockSlice>>,
}

/// Save half memory in comparison to using directly HashSet<BlockHash> while providing enough
/// bytes to reasonably prevent collisions. Use the non-zero part of the hash
struct Seen(HashSet<[u8; 12]>);
impl Seen {
    fn new() -> Seen {
        Seen(HashSet::new())
    }
    fn contains(&self, hash: &BlockHash) -> bool {
        self.0.contains(&hash[..12])
    }
    fn insert(&mut self, hash: &BlockHash) -> bool {
        let key: [u8; 12] = (&hash[..12]).try_into().unwrap();
        self.0.insert(key)
    }
}

impl ReadParse {
    pub fn new(
        blocks_dir: PathBuf,
        network: Network,
        sender: SyncSender<Option<BlockSlice>>,
    ) -> Self {
        ReadParse {
            blocks_dir,
            sender,
            network,
            seen: Seen::new(),
        }
    }

    pub fn start(&mut self) {
        let mut now = Instant::now();
        let mut path = self.blocks_dir.clone();
        path.push("blk*.dat");
        info!("listing block files at {:?}", path);
        let mut paths: Vec<PathBuf> = glob::glob(path.to_str().unwrap())
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        paths.sort();
        info!("There are {} block files", paths.len());
        let mut busy_time = 0u128;
        for path in paths {
            let content = fs::read(&path).unwrap_or_else(|_| panic!("failed to read {:?}", path));
            let content = Arc::new(content);
            let mut cursor = 0usize;
            loop {
                let mut window_cursor = 0usize;
                for (i, window) in content[cursor..].windows(4).enumerate() {
                    let n = u32::from_le_bytes(window.try_into().unwrap());
                    if n != self.network.magic() {
                        window_cursor += 1;
                        continue;
                    }
                    let size_start = cursor + i + 4;
                    let size_end = size_start + 4;
                    if size_end >= content.len() {
                        break;
                    }
                    let size =
                        u32::from_le_bytes(content[size_start..size_end].try_into().unwrap());
                    let block_start = size_end;
                    let block_end = size_end + size as usize;
                    if block_start + 80 >= content.len() {
                        break;
                    }
                    match BlockHeader::consensus_decode(&content[block_start..]) {
                        // We should deserialize the entire block, however we need only
                        // information in the header and we expect the following block
                        // doesn't have serialization issues, thus we save time
                        Ok(header) => {
                            let hash = header.block_hash();
                            if !self.seen.contains(&hash) {
                                self.seen.insert(&hash);
                                let prev = header.prev_blockhash;
                                let block_slice = BlockSlice {
                                    content: Arc::clone(&content),
                                    start: block_start,
                                    end: block_end,
                                    hash,
                                    prev,
                                    next: vec![],
                                };
                                busy_time += now.elapsed().as_nanos();
                                self.sender.send(Some(block_slice)).expect("cannot send");
                                now = Instant::now();
                                cursor = cursor + i + 4 + 4 + size as usize;
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Error in header parsing {:?}", e);
                            cursor = cursor + i + 4 + 4 + 80;
                            break;
                        }
                    }
                }
                // 88 is at least two u32 and an header
                if window_cursor + cursor + 88 >= content.len() {
                    break;
                }
            }
        }
        self.sender.send(None).expect("cannot send");
        info!(
            "ending read parse, busy time: {}s",
            (busy_time / 1_000_000_000)
        );
    }
}

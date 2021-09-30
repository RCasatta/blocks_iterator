use crate::bitcoin::consensus::Decodable;
use crate::bitcoin::{BlockHash, Network};
use crate::FsBlock;
use bitcoin::BlockHeader;
use log::{error, info, warn};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct ReadDetect {
    blocks_dir: PathBuf,
    seen: Seen,
    network: Network,
    sender: SyncSender<Option<FsBlock>>,
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

impl ReadDetect {
    pub fn new(blocks_dir: PathBuf, network: Network, sender: SyncSender<Option<FsBlock>>) -> Self {
        ReadDetect {
            blocks_dir,
            sender,
            seen: Seen::new(),
            network,
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

        // https://github.com/bitcoin/bitcoin/search?q=MAX_BLOCKFILE_SIZE
        let mut content = Vec::with_capacity(0x8000000);

        for path in paths {
            content.clear();
            let mut file = File::open(&path).unwrap();
            file.read_to_end(&mut content).unwrap();
            info!("read {} of {:?}", content.len(), &path);
            let mut cursor = Cursor::new(&content);
            while cursor.position() < content.len() as u64 {
                match u32::consensus_decode(&mut cursor) {
                    Ok(value) => {
                        if self.network.magic() != value {
                            cursor
                                .seek(SeekFrom::Current(-3)) // we advanced by 4 with u32::consensus_decode
                                .expect("failed to seek back");
                            continue;
                        }
                    }
                    Err(_) => break, // EOF
                };
                let size = u32::consensus_decode(&mut cursor).expect("failed to read size");
                let start = cursor.position() as usize;
                match BlockHeader::consensus_decode(&mut cursor) {
                    Ok(header) => {
                        cursor
                            .seek(SeekFrom::Current(i64::from(size - 80))) // remove the parsed header size
                            .expect("failed to seek forward");
                        let hash = header.block_hash();

                        if !self.seen.contains(&hash) {
                            self.seen.insert(&hash);
                            let fs_block = FsBlock {
                                start,
                                end: cursor.position() as usize,
                                path: path.clone(),
                                hash,
                                prev: header.prev_blockhash,
                                next: vec![],
                            };

                            busy_time += now.elapsed().as_nanos();
                            self.sender.send(Some(fs_block)).expect("cannot send");
                            now = Instant::now();
                        } else {
                            warn!("duplicate block {}", hash);
                        }
                    }
                    Err(e) => error!("error header parsing {:?}", e),
                }
            }
        }
        self.sender.send(None).expect("cannot send");
        info!(
            "ending reader parse , busy time: {}s",
            (busy_time / 1_000_000_000)
        );
    }
}

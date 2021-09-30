use crate::read::PathWithContent;
use crate::FsBlock;
use bitcoin::consensus::Decodable;
use bitcoin::{BlockHash, BlockHeader, Network};
use log::{debug, error, info, warn};
use std::collections::HashSet;
use std::convert::TryInto;
use std::io::{Cursor, Seek, SeekFrom};
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Instant;

pub struct Parse {
    network: Network,
    seen: Seen,
    receiver: Receiver<Option<PathWithContent>>,
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

impl Parse {
    pub fn new(
        network: Network,
        receiver: Receiver<Option<PathWithContent>>,
        sender: SyncSender<Option<FsBlock>>,
    ) -> Parse {
        Parse {
            network,
            seen: Seen::new(),
            sender,
            receiver,
        }
    }

    pub fn start(&mut self) {
        let mut total_blocks = 0usize;
        let mut blocks_in_file = 0usize;
        let mut now = Instant::now();
        let mut busy_time = 0u128;
        loop {
            busy_time += now.elapsed().as_nanos();
            let received = self.receiver.recv().expect("cannot receive blob");
            now = Instant::now();
            match received {
                Some(PathWithContent { path, content }) => {
                    let mut cursor = Cursor::new(&content);
                    let max_pos = content.len() as u64;
                    while cursor.position() < max_pos {
                        match u32::consensus_decode(&mut cursor) {
                            Ok(value) => {
                                if self.network.magic() != value {
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

                        match BlockHeader::consensus_decode(&content[start..]) {
                            Ok(header) => {
                                blocks_in_file += 1;
                                let hash = header.block_hash();
                                let fs_block = FsBlock {
                                    start,
                                    end,
                                    path: path.clone(),
                                    hash,
                                    prev: header.prev_blockhash,
                                    next: vec![],
                                };
                                if !self.seen.contains(&hash) {
                                    self.seen.insert(&hash);
                                    busy_time += now.elapsed().as_nanos();
                                    self.sender.send(Some(fs_block)).unwrap();
                                    now = Instant::now();
                                } else {
                                    warn!("duplicate block {}", hash);
                                }
                            }
                            Err(e) => error!("error block parsing {:?}", e),
                        }
                    }

                    total_blocks += blocks_in_file;
                    debug!(
                        "This blob contain {} blocks (total {})",
                        blocks_in_file, total_blocks
                    );
                }
                None => break,
            }
        }

        busy_time += now.elapsed().as_nanos();
        self.sender.send(None).unwrap();
        info!("ending parser, busy time: {}s", (busy_time / 1_000_000_000));
    }
}

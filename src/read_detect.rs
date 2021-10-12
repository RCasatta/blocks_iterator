use crate::bitcoin::consensus::Decodable;
use crate::bitcoin::{BlockHash, Network};
use crate::{periodic_log_level, FsBlock};
use bitcoin::BlockHeader;
use log::{error, info, log, warn};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct ReadDetect {
    blocks_dir: PathBuf,
    seen: Seen,
    network: Network,
    sender: SyncSender<Option<Vec<FsBlock>>>,
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
    pub fn new(
        blocks_dir: PathBuf,
        network: Network,
        sender: SyncSender<Option<Vec<FsBlock>>>,
    ) -> Self {
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
        let mut count = 0u32;

        // Data struct to be reused to read the content of .dat files. We know the max size is 128MiB
        // from https://github.com/bitcoin/bitcoin/search?q=MAX_BLOCKFILE_SIZE
        let mut content = Vec::with_capacity(0x8000000);

        for path in paths {
            content.clear();

            // instead of sending FsBlock on the channel directly, we quickly insert in the vector
            // allowing to read ahead exactly one file (reading no block ahead cause non-parallelizing
            // reading more than 1 file ahead cause cache to work not efficiently)
            let mut fs_blocks = Vec::with_capacity(128);
            let mut rolling = RollingU32::default();

            let mut file = File::open(&path).unwrap();
            file.read_to_end(&mut content).unwrap();
            let file = Arc::new(Mutex::new(file));

            let mut cursor = Cursor::new(&content);
            while cursor.position() < content.len() as u64 {
                match u8::consensus_decode(&mut cursor) {
                    Ok(value) => {
                        rolling.push(value);
                        if self.network.magic() != rolling.as_u32() {
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
                                file: Arc::clone(&file),
                                hash,
                                prev: header.prev_blockhash,
                                next: vec![],
                            };
                            fs_blocks.push(fs_block);
                        } else {
                            warn!("duplicate block {}", hash);
                        }
                    }
                    Err(e) => error!("error header parsing {:?}", e),
                }
            }
            log!(
                periodic_log_level(count, 100),
                "read {} bytes of {:?}, contains {} blocks",
                content.len(),
                &path,
                fs_blocks.len()
            );
            count += 1;

            busy_time += now.elapsed().as_nanos();
            self.sender.send(Some(fs_blocks)).expect("cannot send");
            now = Instant::now();
        }
        info!(
            "ending reader parse , busy time: {}s",
            (busy_time / 1_000_000_000)
        );
        self.sender.send(None).expect("cannot send");
    }
}

#[derive(Default, Debug, Copy, Clone)]
struct RollingU32(u32);
impl RollingU32 {
    fn push(&mut self, byte: u8) {
        self.0 >>= 8;
        self.0 |= (byte as u32) << 24;
    }
    fn as_u32(&self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod test {
    use crate::read_detect::RollingU32;

    #[test]
    fn test_rolling() {
        let mut rolling = RollingU32::default();
        rolling.push(0x0B);
        assert_eq!(
            rolling.as_u32(),
            u32::from_be_bytes([0x0B, 0x00, 0x00, 0x00])
        );
        rolling.push(0x11);
        assert_eq!(
            rolling.as_u32(),
            u32::from_be_bytes([0x11, 0x0b, 0x00, 0x00])
        );
        rolling.push(0x09);
        assert_eq!(
            rolling.as_u32(),
            u32::from_be_bytes([0x09, 0x11, 0x0B, 0x00])
        );
        rolling.push(0x07);
        assert_eq!(
            rolling.as_u32(),
            u32::from_be_bytes([0x07, 0x09, 0x11, 0x0B])
        );
        assert_eq!(rolling.as_u32(), bitcoin::Network::Testnet.magic())
    }

}

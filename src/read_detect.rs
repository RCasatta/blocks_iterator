use crate::bitcoin::consensus::Decodable;
use crate::bitcoin::{Block, BlockHash, Network};
use crate::{periodic_log_level, FsBlock};
use bitcoin::Error;
use log::{error, info, log};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
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
    fn insert(&mut self, hash: &BlockHash) -> bool {
        let key: [u8; 12] = (&hash[..12]).try_into().unwrap();
        self.0.insert(key)
    }
}

pub struct DetectedBlock {
    start: usize,
    end: usize,
    hash: BlockHash,
    prev: BlockHash,
}

impl DetectedBlock {
    fn into_fs_block(self, file: &Arc<Mutex<File>>) -> FsBlock {
        FsBlock {
            start: self.start,
            end: self.end,
            hash: self.hash,
            prev: self.prev,
            file: Arc::clone(file),
            next: vec![],
        }
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

        for (count, path) in paths.into_iter().enumerate() {
            let file = File::open(&path).unwrap();
            let mut reader = BufReader::new(file);
            let detected_blocks = detect(&mut reader, self.network.magic()).unwrap();
            drop(reader);

            let file = File::open(&path).unwrap();
            let file = Arc::new(Mutex::new(file));

            let fs_blocks: Vec<_> = detected_blocks
                .into_iter()
                .filter(|e| self.seen.insert(&e.hash))
                .map(|e| e.into_fs_block(&file))
                .collect();

            // TODO if 0 blocks found, maybe wrong directory

            log!(
                periodic_log_level(count as u32, 100),
                "read {:?}, contains {} blocks",
                &path,
                fs_blocks.len()
            );

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

pub fn detect<R: Read + Seek>(mut reader: &mut R, magic: u32) -> Result<Vec<DetectedBlock>, Error> {
    let mut rolling = RollingU32::default();

    // Instead of sending DetecetdBlock on the channel directly, we quickly insert in the vector
    // allowing to read ahead exactly one file (reading no block ahead cause non-parallelizing
    // reading, more than 1 file ahead cause cache to work not efficiently)
    let mut detected_blocks = Vec::with_capacity(128);

    loop {
        match u8::consensus_decode(&mut reader) {
            Ok(value) => {
                rolling.push(value);
                if magic != rolling.as_u32() {
                    continue;
                }
            }
            Err(_) => break, // EOF
        };
        let size = u32::consensus_decode(&mut reader)?;
        let start = reader.stream_position().unwrap() as usize;
        match Block::consensus_decode(&mut reader) {
            Ok(block) => {
                let end = reader.stream_position().unwrap() as usize;
                assert_eq!(size as usize, end - start);
                let hash = block.header.block_hash();
                let detected_block = DetectedBlock {
                    start,
                    end,
                    hash,
                    prev: block.header.prev_blockhash,
                };
                detected_blocks.push(detected_block);
            }
            Err(e) => {
                // It's mandatory to use stream_position (require MSRV 1.51) because I can't maintain
                // a byte read position because in case of error I don't know how many bytes of the
                // reader has been consumed
                error!("error block parsing {:?}", e)
            }
        }
    }
    Ok(detected_blocks)
}

/// Implements a rolling u32, every time a new u8 is `push`ed the old value is shifted by 1 byte
/// Allows to read a stream searching for a u32 magic without going back
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

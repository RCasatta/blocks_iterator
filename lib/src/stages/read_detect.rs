use crate::bitcoin::{BlockHash, Network};
use crate::{FsBlock, Periodic};
use bitcoin::hashes::Hash;
use bitcoin::p2p::Magic;
use bitcoin_slices::number::{U32, U8};
use bitcoin_slices::{bsl, Parse};
use log::info;
use std::collections::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct ReadDetect {
    join: Option<JoinHandle<()>>,
}

impl Drop for ReadDetect {
    fn drop(&mut self) {
        if let Some(jh) = self.join.take() {
            jh.join().expect("thread failed");
        }
    }
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
    fn into_fs_block(self, file: &Arc<Mutex<File>>, serialization_version: u8) -> FsBlock {
        FsBlock {
            start: self.start,
            end: self.end,
            hash: self.hash,
            prev: self.prev,
            file: Arc::clone(file),
            next: vec![],
            serialization_version,
        }
    }
}

impl ReadDetect {
    pub fn new(
        blocks_dir: PathBuf,
        network: Network,
        early_stop: Arc<AtomicBool>,
        sender: SyncSender<Option<Vec<FsBlock>>>,
        serialization_version: u8,
    ) -> Self {
        let mut periodic = Periodic::new(Duration::from_secs(60));
        let mut vec = Vec::with_capacity(135_000_000);
        Self {
            join: Some(std::thread::spawn(move || {
                info!("starting read_detect");

                let mut now = Instant::now();
                let mut seen = Seen::new();
                let mut path = blocks_dir.clone();
                path.push("blk*.dat");
                info!("listing block files at {:?}", path);
                let mut paths: Vec<PathBuf> = glob::glob(path.to_str().unwrap())
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                paths.sort();
                info!("There are {} block files", paths.len());
                let mut busy_time = 0u128;

                for path in paths.into_iter() {
                    let mut file = File::open(&path).unwrap();
                    file.read_to_end(&mut vec).unwrap();
                    let detected_blocks = detect(&vec, network.magic()).unwrap();
                    vec.clear();
                    drop(file);

                    let file = File::open(&path).unwrap();
                    let file = Arc::new(Mutex::new(file));

                    let fs_blocks: Vec<_> = detected_blocks
                        .into_iter()
                        .filter(|e| seen.insert(&e.hash))
                        .map(|e| e.into_fs_block(&file, serialization_version))
                        .collect();

                    // TODO if 0 blocks found, maybe wrong directory
                    if periodic.elapsed() {
                        info!("read {:?}, contains {} blocks", &path, fs_blocks.len());
                    }

                    busy_time += now.elapsed().as_nanos();
                    if early_stop.load(Ordering::Relaxed) {
                        break;
                    } else {
                        sender.send(Some(fs_blocks)).expect("cannot send");
                    }

                    now = Instant::now();
                }
                info!(
                    "ending read_detect , busy time: {}s",
                    (busy_time / 1_000_000_000)
                );
                if !early_stop.load(Ordering::Relaxed) {
                    info!("sending None");
                    sender.send(None).expect("cannot send");
                }
            })),
        }
    }
}

pub fn detect(buffer: &[u8], magic: Magic) -> Result<Vec<DetectedBlock>, bitcoin_slices::Error> {
    let mut pointer = 0usize;
    let mut rolling = RollingU32::default();
    let magic_u32 = u32::from_le_bytes(magic.to_bytes());

    // Instead of sending DetecetdBlock on the channel directly, we quickly insert in the vector
    // allowing to read ahead exactly one file (reading no block ahead cause non-parallelizing
    // reading, more than 1 file ahead cause cache to work not efficiently)
    let mut detected_blocks = Vec::with_capacity(128);
    let mut current = buffer;
    while let Ok(byte) = U8::parse(current) {
        pointer += 1;
        current = byte.remaining();
        rolling.push(byte.parsed().into());
        if magic_u32 != rolling.as_u32() {
            continue;
        }

        let size = U32::parse(current)?;
        let remaining = size.remaining();
        let size: u32 = size.parsed().into();
        pointer += 4;
        let start = pointer;
        match bsl::Block::parse(remaining) {
            Ok(block) => {
                pointer += block.consumed();
                current = block.remaining();
                let end = pointer;
                let hash = BlockHash::from_slice(&block.parsed().block_hash_sha2()[..]).unwrap();
                let prev = BlockHash::from_slice(block.parsed().header().prev_blockhash()).unwrap();
                if size as usize != end - start {
                    continue;
                }

                let detected_block = DetectedBlock {
                    start,
                    end,
                    hash,
                    prev,
                };
                detected_blocks.push(detected_block);
            }
            Err(_) => continue,
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
    use crate::stages::read_detect::RollingU32;

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
        assert_eq!(
            rolling.as_u32(),
            u32::from_le_bytes(bitcoin::Network::Testnet.magic().to_bytes())
        )
    }
}

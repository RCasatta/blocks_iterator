use crate::bitcoin::consensus::Decodable;
use crate::bitcoin::{BlockHash, Network};
use crate::{FsBlock, Periodic};
use bitcoin::{BlockHeader, Error, Transaction};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;
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

#[derive(Clone, Serialize, Deserialize)]
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
        detected_blocks_cache: PathBuf,
        network: Network,
        sender: SyncSender<Option<Vec<FsBlock>>>,
    ) -> Self {
        Self {
            join: Some(std::thread::spawn(move || {
                let mut periodic = Periodic::new(Duration::from_secs(60));
                let mut detected_cache: HashMap<BlockHash, DetectedBlock> =
                    match File::open(&detected_blocks_cache) {
                        Ok(file) => {
                            serde_cbor::from_reader(BufReader::new(file)).unwrap_or_default()
                        }
                        Err(_) => {
                            warn!("detected cache not found");
                            HashMap::default()
                        }
                    };

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
                    let file = File::open(&path).unwrap();
                    let mut reader = BufReader::new(file);
                    let detected_blocks =
                        detect(&mut reader, network.magic(), &mut detected_cache).unwrap();
                    drop(reader);

                    let file = File::open(&path).unwrap();
                    let file = Arc::new(Mutex::new(file));

                    let fs_blocks: Vec<_> = detected_blocks
                        .into_iter()
                        .filter(|e| seen.insert(&e.hash))
                        .map(|e| e.into_fs_block(&file))
                        .collect();

                    // TODO if 0 blocks found, maybe wrong directory
                    if periodic.elapsed() {
                        info!("read {:?}, contains {} blocks", &path, fs_blocks.len());
                    }

                    busy_time += now.elapsed().as_nanos();
                    sender.send(Some(fs_blocks)).expect("cannot send");
                    now = Instant::now();
                }

                serde_cbor::to_writer(
                    File::create(&detected_blocks_cache).unwrap(),
                    &detected_cache,
                )
                .unwrap();
                info!(
                    "ending reader parse , busy time: {}s",
                    (busy_time / 1_000_000_000)
                );
                sender.send(None).expect("cannot send");
            })),
        }
    }
}

pub fn detect<R: Read + Seek>(
    mut reader: &mut R,
    magic: u32,
    detected_cache: &mut HashMap<BlockHash, DetectedBlock>,
) -> Result<Vec<DetectedBlock>, Error> {
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
        match BlockHeader::consensus_decode(&mut reader) {
            Ok(block_header) => {
                let block_hash = block_header.block_hash();
                match detected_cache.get(&block_hash) {
                    Some(detected_block) => {
                        reader
                            .seek(SeekFrom::Current(
                                (detected_block.end - detected_block.start - 80) as i64,
                            ))
                            .unwrap(); // TODO remove unwrap
                        detected_blocks.push(detected_block.clone());
                    }
                    None => match Vec::<Transaction>::consensus_decode(&mut reader) {
                        Ok(_) => {
                            let end = reader.stream_position().unwrap() as usize;
                            assert_eq!(size as usize, end - start);
                            let detected_block = DetectedBlock {
                                start,
                                end,
                                hash: block_hash,
                                prev: block_header.prev_blockhash,
                            };
                            detected_cache.insert(block_hash, detected_block.clone());
                            detected_blocks.push(detected_block);
                        }
                        Err(e) => {
                            error!("error block parsing {:?}", e)
                        }
                    },
                }
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

//! # Blocks Iterator
//!
//! Read bitcoin blocks directory containing `blocks*.dat` files, and produce a ordered stream
//! of [BlockExtra]
//!

// Coding conventions
#![forbid(unsafe_code)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(dead_code)]
#![deny(unused_imports)]
#![deny(missing_docs)]
#![deny(unused_must_use)]
#![cfg_attr(all(test, feature = "unstable"), feature(test))]
#[cfg(all(test, feature = "unstable"))]
extern crate test;

use bitcoin::BlockHash;
use log::{error, info, Level};
use std::fs::File;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;
use structopt::StructOpt;
use utxo::AnyUtxo;

pub use period::{PeriodCounter, Periodic};

mod block_extra;
mod fee;
mod period;
mod pipe;
mod read_detect;
mod reorder;
mod utxo;

// re-exporting deps
pub use bitcoin;
pub use fxhash;
pub use glob;
pub use log;
pub use structopt;

pub use block_extra::BlockExtra;
pub use pipe::PipeIterator;

/// Configuration parameters, most important the bitcoin blocks directory
#[derive(StructOpt, Debug, Clone)]
pub struct Config {
    /// Blocks directory (containing `blocks*.dat`)
    #[structopt(short, long)]
    pub blocks_dir: PathBuf,

    /// Network (bitcoin, testnet, regtest, signet)
    #[structopt(short, long)]
    pub network: bitcoin::Network,

    /// Skip calculation of previous outputs, it's faster and it uses much less memory
    /// however make it impossible calculate fees or access tx input previous scripts
    #[structopt(short, long)]
    pub skip_prevout: bool,

    /// Maximum length of a reorg allowed, during reordering send block to the next step only
    /// if it has `max_reorg` following blocks. Higher is more conservative, while lower faster.
    /// When parsing testnet blocks, it may be necessary to increase this a lot
    #[structopt(short, long, default_value = "6")]
    pub max_reorg: u8,

    /// Size of the channels used to pass messages between threads
    #[structopt(short, long, default_value = "0")]
    pub channels_size: u8,

    #[cfg(feature = "db")]
    /// Specify a directory where a database will be created to store the Utxo (when `--skip-prevout` is not used)
    /// Reduce the memory requirements but it's slower and use disk space
    #[structopt(short, long)]
    pub utxo_db: Option<PathBuf>,

    /// Stop the blocks iteration at the specified height
    #[structopt(long)]
    pub stop_at_height: Option<u32>,
}

impl Config {
    #[cfg(not(feature = "db"))]
    fn utxo_manager(&self) -> AnyUtxo {
        AnyUtxo::Mem(utxo::MemUtxo::new(self.network))
    }
    #[cfg(feature = "db")]
    fn utxo_manager(&self) -> AnyUtxo {
        match &self.utxo_db {
            Some(path) => AnyUtxo::Db(utxo::DbUtxo::new(path).unwrap()), //TODO unwrap
            None => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        }
    }
}

/// Before reorder we keep only the position of the block in the file system and data relative
/// to the block hash, the previous hash and the following hash (populated during reorder phase)
/// We will need to read the block from disk again, but by doing so we will avoid using too much
/// memory in the [`OutOfOrderBlocks`] map.
#[derive(Debug)]
pub struct FsBlock {
    /// the file the block identified by `hash` is stored in. Multiple blocks are stored in the
    /// and we don't want to open/close the file many times for performance reasons so it's shared.
    /// It's a Mutex to allow to be sent between threads but only one thread (reorder) mutably
    /// access to it so there is no contention. (Arc alone isn't enough cause it can't be mutated,
    /// RefCell can be mutated but not sent between threads)
    pub file: Arc<Mutex<File>>,

    /// The start position in bytes in the `file` at which the block identified by `hash`
    pub start: usize,

    /// The end position in bytes in the `file` at which the block identified by `hash`
    pub end: usize,

    /// The hash identifying this block, output of `block.header.block_hash()`
    pub hash: BlockHash,

    /// The hash of the block previous to this one, `block.header.prev_blockhash`
    pub prev: BlockHash,

    /// The hash of the blocks following this one. It is populated during the reorder phase, it can
    /// be more than one because of reorgs.
    pub next: Vec<BlockHash>,
}

struct BlockExtraIterator {
    handle: Option<JoinHandle<()>>,
    recv: Receiver<Option<BlockExtra>>,
}
impl Iterator for BlockExtraIterator {
    type Item = BlockExtra;

    fn next(&mut self) -> Option<Self::Item> {
        match self.recv.recv() {
            Ok(Some(val)) => Some(val),
            Ok(None) => {
                if let Some(handle) = self.handle.take() {
                    handle.join().unwrap();
                }
                None
            }
            Err(e) => {
                error!("error iterating {:?}", e);
                if let Some(handle) = self.handle.take() {
                    handle.join().unwrap();
                }
                None
            }
        }
    }
}

/// Return an Iterator of [`BlockExtra`] read from `blocks*.dat` contained in the `config.blocks_dir`
/// Blocks returned are ordered from the genesis to the highest block in the directory
/// (minus `config.max_reorg`).
pub fn iter(config: Config) -> impl Iterator<Item = BlockExtra> {
    let (send, recv) = sync_channel(config.channels_size.into());

    let handle = Some(iterate(config, send));

    BlockExtraIterator { handle, recv }
}

#[cfg(feature = "parallel")]
/// Parallelized iterator
pub fn par_iter<STATE, PREPROC, TASK, DATA>(
    config: Config,
    state: STATE,
    pre_processing: PREPROC,
    task: TASK,
) where
    PREPROC: Fn(BlockExtra) -> Vec<DATA> + 'static + Send,
    TASK: Fn(DATA, Arc<STATE>) -> bool + 'static + Send + Sync,
    DATA: 'static + Send,
    STATE: 'static + Send + Sync,
{
    use log::debug;
    use rayon::prelude::*;
    use std::sync::atomic::Ordering::SeqCst;

    let (send_task, recv_task) = sync_channel(config.channels_size.into());
    let stop = Arc::new(AtomicBool::new(false));
    let state = Arc::new(state);

    let stop_clone = stop.clone();
    let iter = iter(config);

    let handle = thread::spawn(move || {
        debug!("start pre-processing thread");
        for block_extra in iter {
            if stop_clone.load(SeqCst) {
                break;
            }
            let data_vec = pre_processing(block_extra);
            for data in data_vec {
                send_task.send(data).unwrap();
            }
        }
        debug!("ending pre-processing thread");
    });

    recv_task.into_iter().par_bridge().for_each(|data: DATA| {
        debug!("iter");
        let result = task(data, state.clone());

        if result {
            stop.store(true, SeqCst)
        }
    });
    handle.join().unwrap();
}

fn iterate(config: Config, channel: SyncSender<Option<BlockExtra>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let now = Instant::now();
        let early_stop = Arc::new(AtomicBool::new(false));

        // FsBlock is a small struct (~120b), so 10_000 is not a problem but allows the read_detect to read ahead the next block file
        let (send_block_fs, receive_block_fs) = sync_channel(0);
        let _read = read_detect::ReadDetect::new(
            config.blocks_dir.clone(),
            config.network,
            early_stop.clone(),
            send_block_fs,
        );

        let (send_ordered_blocks, receive_ordered_blocks) =
            sync_channel(config.channels_size.into());
        let send_ordered_blocks = if config.skip_prevout {
            // if skip_prevout is true, we send directly to end step
            channel.clone()
        } else {
            send_ordered_blocks
        };
        let _reorder = reorder::Reorder::new(
            config.network,
            config.max_reorg,
            config.stop_at_height,
            early_stop,
            receive_block_fs,
            send_ordered_blocks,
        );

        if !config.skip_prevout {
            let _fee = fee::Fee::new(receive_ordered_blocks, channel, config.utxo_manager());
        }

        info!("Total time elapsed: {}s", now.elapsed().as_secs());
    })
}

/// Utility method usually returning [log::Level::Debug] but when `i` is divisible by `every` returns [log::Level::Info]
#[deprecated]
pub fn periodic_log_level(i: u32, every: u32) -> Level {
    if i % every == 0 {
        Level::Info
    } else {
        Level::Debug
    }
}

#[cfg(test)]
mod inner_test {
    use crate::bitcoin::Network;
    use crate::{iterate, Config};
    use std::sync::mpsc::sync_channel;

    #[test]
    fn test_blk_testnet() {
        let conf = Config {
            blocks_dir: "../blocks".into(),
            network: Network::Testnet,
            skip_prevout: false,
            max_reorg: 10,
            channels_size: 0,
            #[cfg(feature = "db")]
            utxo_db: None,
            stop_at_height: None,
        };
        let (send, recv) = sync_channel(0);

        let handle = iterate(conf, send);
        while let Some(b) = recv.recv().unwrap() {
            if b.height == 394 {
                assert_eq!(b.fee(), Some(50_000));
            }
        }
        handle.join().unwrap();
    }

    #[cfg(feature = "db")]
    #[test]
    fn test_blk_testnet_db() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let conf = Config {
            blocks_dir: "../blocks".into(),
            network: Network::Testnet,
            skip_prevout: false,
            max_reorg: 10,
            channels_size: 0,
            stop_at_height: None,
            utxo_db: Some(tempdir.path().to_path_buf()),
        };
        let (send, recv) = sync_channel(0);

        let handle = iterate(conf.clone(), send);
        while let Some(b) = recv.recv().unwrap() {
            if b.height == 394 {
                assert_eq!(b.fee(), Some(50_000));
            }
        }
        handle.join().unwrap();

        // iterating twice, this time prevouts come directly from db
        let (send, recv) = sync_channel(0);
        let handle = iterate(conf, send);
        while let Some(b) = recv.recv().unwrap() {
            if b.height == 394 {
                assert_eq!(b.fee(), Some(50_000));
            }
        }
        handle.join().unwrap();
    }
}

#[cfg(all(test, feature = "unstable"))]
mod bench {
    use crate::bitcoin::hashes::{sha256, Hash, HashEngine};
    use crate::bitcoin::OutPoint;
    use bitcoin::hashes::sha256::Midstate;
    use sha2::{Digest, Sha256};
    use test::{black_box, Bencher};

    #[bench]
    fn bench_blake3(b: &mut Bencher) {
        let outpoint = OutPoint::default();
        let salt = [0u8; 12];

        b.iter(|| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&salt[..]);
            hasher.update(outpoint.txid.as_ref());
            hasher.update(&outpoint.vout.to_ne_bytes());
            let hash = hasher.finalize();
            let mut result = [0u8; 12];
            result.copy_from_slice(&hash.as_bytes()[..12]);
            result
        });
    }

    #[bench]
    fn bench_bitcoin_hashes_sha(b: &mut Bencher) {
        let outpoint = OutPoint::default();
        let salt = [0u8; 12];

        b.iter(|| {
            let mut engine = sha256::Hash::engine();
            engine.input(&salt);
            engine.input(&outpoint.txid.as_ref());
            engine.input(&outpoint.vout.to_ne_bytes()[..]);
            let hash = sha256::Hash::from_engine(engine);
            let mut result = [0u8; 12];
            result.copy_from_slice(&hash.into_inner()[..12]);
            black_box(result);
        });
    }

    #[bench]
    fn bench_bitcoin_hashes_sha_midstate(b: &mut Bencher) {
        let outpoint = OutPoint::default();
        let salt = [0u8; 32];
        let midstate = Midstate(salt);
        let midstate_engine = sha256::HashEngine::from_midstate(midstate, 64);
        b.iter(|| {
            let mut engine = midstate_engine.clone();
            engine.input(&outpoint.txid.as_ref());
            engine.input(&outpoint.vout.to_ne_bytes()[..]);
            let hash = sha256::Hash::from_engine(engine);
            let mut result = [0u8; 12];
            result.copy_from_slice(&hash.into_inner()[..12]);
            black_box(result);
        });
    }

    #[bench]
    fn bench_sha2_crate(b: &mut Bencher) {
        let outpoint = OutPoint::default();
        let salt = [0u8; 12];

        b.iter(|| {
            let mut hasher = Sha256::new();
            hasher.update(&salt);
            hasher.update(&outpoint.txid.as_ref());
            hasher.update(&outpoint.vout.to_ne_bytes()[..]);
            let hash = hasher.finalize();
            let mut result = [0u8; 12];
            result.copy_from_slice(&hash[..12]);
            black_box(result);
        });
    }

    #[bench]
    fn bench_bitcoin_hashes_sha_long(b: &mut Bencher) {
        let a: Vec<_> = (0u8..255).cycle().take(1000).collect();
        b.iter(|| {
            let mut engine = sha256::Hash::engine();
            engine.input(&a);
            let hash = sha256::Hash::from_engine(engine);
            black_box(hash);
        });
    }

    #[bench]
    fn bench_sha2_crate_long(b: &mut Bencher) {
        let a: Vec<_> = (0u8..255).cycle().take(1000).collect();
        b.iter(|| {
            let mut hasher = Sha256::new();
            hasher.update(&a);
            let hash = hasher.finalize();
            black_box(hash);
        });
    }

    #[bench]
    fn bench_fxhash(b: &mut Bencher) {
        let outpoint = OutPoint::default();
        let salt = [0u8; 12];

        b.iter(|| {
            let a = fxhash::hash32(&(&outpoint, &salt));
            let b = fxhash::hash64(&(&outpoint, &salt));
            let mut result = [0u8; 12];

            result[..4].copy_from_slice(&a.to_ne_bytes()[..]);
            result[4..].copy_from_slice(&b.to_ne_bytes()[..]);
            black_box(result);
        });
    }
}

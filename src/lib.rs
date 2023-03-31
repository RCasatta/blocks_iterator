#![doc = include_str!("../README.md")]
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
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#[cfg(all(test, feature = "unstable"))]
extern crate test;

use bitcoin::BlockHash;
use log::{info, Level};
use std::fs::File;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;
use structopt::StructOpt;
use utxo::AnyUtxo;

pub use period::{PeriodCounter, Periodic};

mod block_extra;
mod bsl;
mod iter;
mod period;
mod pipe;
mod stages;
mod utxo;

// re-exporting deps
pub use bitcoin;
pub use fxhash;
pub use glob;
pub use log;
pub use structopt;

pub use block_extra::BlockExtra;
pub use iter::iter;
pub use pipe::PipeIterator;

#[allow(deprecated)]
pub use iter::par_iter;

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
    /// Specify a **directory** where a rocks database will be created to store the Utxo (when `--skip-prevout` is not used)
    /// Reduce the memory requirements but it's slower and use disk space
    #[structopt(short, long)]
    pub utxo_db: Option<PathBuf>,

    /// Specify a **file** where a redb database will be created to store the Utxo (when `--skip-prevout` is not used)
    /// Reduce the memory requirements but it's slower and use disk space.
    ///
    /// Note with feature db you also have the options to use rocksdb, which is faster during creation of the utxo set
    /// but slower to compile.
    #[structopt(short, long)]
    pub utxo_redb: Option<PathBuf>,

    /// Start the blocks iteration at the specified height, note blocks*.dat file are read and
    /// analyzed anyway to follow the blockchain starting at the genesis and populate utxos,
    /// however they are not emitted
    #[structopt(long, default_value = "0")]
    pub start_at_height: u32,

    /// Stop the blocks iteration at the specified height
    #[structopt(long)]
    pub stop_at_height: Option<u32>,
}

impl Config {
    #[cfg(not(feature = "db"))]
    fn utxo_manager(&self) -> AnyUtxo {
        match &self.utxo_redb {
            Some(path) => AnyUtxo::Redb(utxo::RedbUtxo::new(path).unwrap()),
            None => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        }
    }
    #[cfg(feature = "db")]
    fn utxo_manager(&self) -> AnyUtxo {
        match (&self.utxo_db, &self.utxo_redb) {
            (Some(_), Some(_)) => panic!("utxo_db and utxo_redb cannot be specified together"),
            (Some(path), None) => AnyUtxo::Db(utxo::DbUtxo::new(path).unwrap()), //TODO unwrap
            (None, Some(path)) => AnyUtxo::Redb(utxo::RedbUtxo::new(path).unwrap()), //TODO unwra
            (None, None) => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        }
    }
}

/// Before reorder we keep only the position of the block in the file system and data relative
/// to the block hash, the previous hash and the following hash (populated during reorder phase)
/// We will need
///  to read the block from disk again, but by doing so we will avoid using too much
/// memory in the `OutOfOrderBlocks` map.
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

fn iterate(config: Config, channel: SyncSender<Option<BlockExtra>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let now = Instant::now();
        let early_stop = Arc::new(AtomicBool::new(false));

        // FsBlock is a small struct (~120b), so 10_000 is not a problem but allows the read_detect to read ahead the next block file
        let (send_block_fs, receive_block_fs) = sync_channel(0);
        let _read = stages::ReadDetect::new(
            config.blocks_dir.clone(),
            config.network,
            early_stop.clone(),
            send_block_fs,
        );

        let (send_ordered_blocks, receive_ordered_blocks) =
            sync_channel(config.channels_size.into());
        let _reorder = stages::Reorder::new(
            config.network,
            config.max_reorg,
            config.stop_at_height,
            early_stop,
            receive_block_fs,
            send_ordered_blocks,
        );

        let (send_blocks_with_txids, receive_blocks_with_txids) =
            sync_channel(config.channels_size.into());
        let send_blocks_with_txids = if config.skip_prevout {
            // if skip_prevout is true, we send directly to end step
            channel.clone()
        } else {
            send_blocks_with_txids
        };

        let _compute_txids = stages::ComputeTxids::new(
            config.skip_prevout,
            config.start_at_height,
            receive_ordered_blocks,
            send_blocks_with_txids,
        );

        if !config.skip_prevout {
            let _fee = stages::Fee::new(
                config.start_at_height,
                receive_blocks_with_txids,
                channel,
                config.utxo_manager(),
            );
        }

        info!("Total time elapsed: {}s", now.elapsed().as_secs());
    })
}

/// Utility method usually returning [log::Level::Debug] but when `i` is divisible by `every` returns [log::Level::Info]
#[deprecated(note = "use `period::Periodic` or `period::PeriodCounter`")]
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

    pub fn test_conf() -> Config {
        Config {
            blocks_dir: "blocks".into(),
            network: Network::Testnet,
            skip_prevout: false,
            max_reorg: 10,
            channels_size: 0,
            #[cfg(feature = "db")]
            utxo_db: None,
            start_at_height: 0,
            stop_at_height: None,
            utxo_redb: None,
        }
    }

    #[test]
    fn test_blk_testnet() {
        let _ = env_logger::try_init();

        let conf = test_conf();
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
        let _ = env_logger::try_init();

        let tempdir = tempfile::TempDir::new().unwrap();
        let conf = {
            let mut conf = test_conf();
            conf.utxo_db = Some(tempdir.path().to_path_buf());
            conf
        };

        let mut max_height = 0;
        for b in super::iter(conf.clone()) {
            max_height = max_height.max(b.height);
            if b.height == 389 {
                assert_eq!(b.fee(), Some(50_000));
                assert_eq!(b.iter_tx().size_hint(), (2, Some(2)));
            }
            assert!(b.iter_tx().next().is_some());
            for (txid, tx) in b.iter_tx() {
                assert_eq!(*txid, tx.txid());
            }
        }
        assert_eq!(max_height, 390);

        // iterating twice, this time prevouts come directly from db
        for b in super::iter(conf) {
            if b.height == 394 {
                assert_eq!(b.fee(), Some(50_000));
            }
        }
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

#![doc = include_str!("../README.md")]
// Coding conventions
#![forbid(unsafe_code)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(dead_code)]
#![deny(unused_imports)]
#![deny(unused_must_use)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use bitcoin::BlockHash;
use log::{info, Level};
use std::fs::File;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;

pub use period::{PeriodCounter, Periodic};

mod block_extra;
mod bsl;
mod config;
mod error;
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
pub use config::Config;
pub use error::Error;
pub use iter::iter;
pub use pipe::PipeIterator;

#[allow(deprecated)]
pub use iter::par_iter;

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
            early_stop.clone(),
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
            match config.utxo_manager() {
                Ok(utxo_manager) => {
                    let _fee = stages::Fee::new(
                        config.start_at_height,
                        receive_blocks_with_txids,
                        channel,
                        utxo_manager,
                    );
                }
                Err(e) => {
                    log::error!("{e}");
                    early_stop.store(true, Ordering::Relaxed);
                    channel.send(None).unwrap();
                }
            }
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
        Config::new("blocks", Network::Testnet)
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
        assert_eq!(max_height, 400 - conf.max_reorg as u32);

        // iterating twice, this time prevouts come directly from db
        for b in super::iter(conf) {
            if b.height == 394 {
                assert_eq!(b.fee(), Some(50_000));
            }
        }
    }
}

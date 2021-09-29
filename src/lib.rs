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
//#![deny(missing_docs)]
#![deny(unused_must_use)]

use bitcoin::{Block, BlockHash, OutPoint, Transaction, TxOut};
use log::{info, Level};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;
use structopt::StructOpt;

mod fee;
mod read_parse;
mod reorder;
mod truncmap;

// re-exporting deps
pub use bitcoin;
use bitcoin::consensus::Decodable;
pub use fxhash;
pub use glob;
pub use log;
use std::sync::Arc;
pub use structopt;

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
}

/// The bitcoin block and additional metadata returned by the [iterate] method
#[derive(Debug)]
pub struct BlockExtra {
    /// The bitcoin block
    pub block: Block,
    /// The bitcoin block hash, same as `block.block_hash()` but result from hashing is cached
    pub block_hash: BlockHash,
    /// The byte size of the block, as returned by in `serialize(block).len()`
    pub size: u32,
    /// Hash of the blocks following this one, it's a vec because during reordering they may be more
    /// than one because of reorgs, as a result from [iterate], it's just one.
    pub next: Vec<BlockHash>,
    /// The height of the current block, number of blocks between this one and the genesis block
    pub height: u32,
    /// All the previous outputs of this block. Allowing to validate the script or computing the fee
    /// Note that when configuration `skip_script_pubkey` is true, the script is empty,
    /// when `skip_prevout` is true, this map is empty.
    pub outpoint_values: HashMap<OutPoint, TxOut>,
}

/// Before reorder we keep only the position of the block in the file system and data relative
/// to the block hash, the previous hash and the following hash (populated during reorder phase)
/// We will need to read the block from disk again, but by doing so we will avoid using too much
/// memory in the [`OutOfOrderBlocks`] map.
#[derive(Debug)]
pub struct BlockSlice {
    pub content: Arc<Vec<u8>>,
    pub start: usize,
    pub end: usize,
    pub hash: BlockHash,
    pub prev: BlockHash,
    pub next: Vec<BlockHash>,
}

impl TryFrom<BlockSlice> for BlockExtra {
    type Error = bitcoin::Error;

    fn try_from(block_slice: BlockSlice) -> Result<Self, Self::Error> {
        Ok(BlockExtra {
            block: Block::consensus_decode(
                &block_slice.content[block_slice.start..block_slice.end],
            )?,
            block_hash: block_slice.hash,
            size: (block_slice.end - block_slice.start) as u32,
            next: block_slice.next,
            height: 0,
            outpoint_values: Default::default(),
        })
    }
}

impl BlockExtra {
    /// Returns the average transaction fee in the block
    pub fn average_fee(&self) -> Option<f64> {
        Some(self.fee()? as f64 / self.block.txdata.len() as f64)
    }

    /// Returns the total fee of the block
    pub fn fee(&self) -> Option<u64> {
        let mut total = 0u64;
        for tx in self.block.txdata.iter() {
            total += self.tx_fee(tx)?;
        }
        Some(total)
    }

    /// Returns the fee of a transaction contained in the block
    pub fn tx_fee(&self, tx: &Transaction) -> Option<u64> {
        let output_total: u64 = tx.output.iter().map(|el| el.value).sum();
        let mut input_total = 0u64;
        for input in tx.input.iter() {
            input_total += self.outpoint_values.get(&input.previous_output)?.value;
        }
        Some(input_total - output_total)
    }
}

/// Read `blocks*.dat` contained in the `config.blocks_dir` directory and returns [BlockExtra]
/// through a channel supplied from the caller. Blocks returned are ordered from the genesis to the
/// highest block in the directory (minus `config.max_reorg`).
/// In this call threads are spawned, caller must call [std::thread::JoinHandle::join] on the returning handle.
pub fn iterate(config: Config, channel: SyncSender<Option<BlockExtra>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let now = Instant::now();

        let (send_slice, receive_slice) = sync_channel(config.channels_size.into());

        let mut read_parse =
            read_parse::ReadParse::new(config.blocks_dir.clone(), config.network, send_slice);
        let read_parse_handle = thread::spawn(move || {
            read_parse.start();
        });

        let (send_ordered_blocks, receive_ordered_blocks) =
            sync_channel(config.channels_size.into());
        let send_ordered_blocks = if config.skip_prevout {
            // if skip_prevout is true, we send directly to end step
            channel.clone()
        } else {
            send_ordered_blocks
        };
        let mut reorder = reorder::Reorder::new(
            config.network,
            config.max_reorg,
            receive_slice,
            send_ordered_blocks,
        );
        let orderer_handle = thread::spawn(move || {
            reorder.start();
        });

        if !config.skip_prevout {
            let mut fee = fee::Fee::new(receive_ordered_blocks, channel);
            let fee_handle = thread::spawn(move || {
                fee.start();
            });
            fee_handle.join().unwrap();
        }

        read_parse_handle.join().unwrap();
        orderer_handle.join().unwrap();
        info!("Total time elapsed: {}s", now.elapsed().as_secs());
    })
}

/// Utility method usually returning [log::Level::Debug] but when `i` is divisible by `10_000` returns [log::Level::Info]
pub fn periodic_log_level(i: u32) -> Level {
    if i % 10_000 == 0 {
        Level::Info
    } else {
        Level::Debug
    }
}

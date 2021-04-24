use bitcoin::{Block, BlockHash, OutPoint, Transaction, TxOut, Txid};
use log::{info, Level};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;
use structopt::StructOpt;

mod fee;
mod parse;
mod read;
mod reorder;
mod truncmap;

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

    /// Doesn't store the script_pubkey so it's not available in previous_output to save memory.
    /// When `skip_prevout` is true it's implied
    #[structopt(short, long)]
    pub skip_script_pubkey: bool,

    /// Maximum length of a reorg allowed, during reordering send block to the next step only
    /// if it has `max_reorg` following blocks. Higher is more conservative, while lower faster.
    /// When parsing testnet blocks, it may be necessary to increase this a lot
    #[structopt(short, long, default_value = "6")]
    pub max_reorg: u8,
}

#[derive(Debug)]
pub struct BlockExtra {
    pub block: Block,
    pub block_hash: BlockHash,
    pub size: u32,
    pub next: Vec<BlockHash>, // vec cause in case of reorg could be more than one
    pub height: u32,
    pub outpoint_values: HashMap<OutPoint, TxOut>,
    pub tx_hashes: HashSet<Txid>,
}

impl BlockExtra {
    pub fn average_fee(&self) -> f64 {
        self.fee() as f64 / self.block.txdata.len() as f64
    }

    pub fn fee(&self) -> u64 {
        let mut total = 0u64;
        for tx in self.block.txdata.iter() {
            total += self.tx_fee(tx);
        }
        total
    }

    pub fn tx_fee(&self, tx: &Transaction) -> u64 {
        let output_total: u64 = tx.output.iter().map(|el| el.value).sum();
        let mut input_total = 0u64;
        for input in tx.input.iter() {
            match self.outpoint_values.get(&input.previous_output) {
                Some(txout) => input_total += txout.value,
                None => panic!("can't find tx fee {}", tx.txid()),
            }
        }
        input_total - output_total
    }
}

pub fn iterate(config: Config, channels: SyncSender<Option<BlockExtra>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let now = Instant::now();

        let (send_blobs, receive_blobs) = sync_channel(2);

        let mut read = read::Read::new(config.blocks_dir.clone(), send_blobs);
        let read_handle = thread::spawn(move || {
            read.start();
        });

        let (send_blocks, receive_blocks) = sync_channel(200);
        let mut parse = parse::Parse::new(config.network, receive_blobs, send_blocks);
        let parse_handle = thread::spawn(move || {
            parse.start();
        });

        let (send_ordered_blocks, receive_ordered_blocks) = sync_channel(200);
        let mut reorder = reorder::Reorder::new(
            config.network,
            config.max_reorg,
            receive_blocks,
            send_ordered_blocks,
        );
        let orderer_handle = thread::spawn(move || {
            reorder.start();
        });

        let mut fee = fee::Fee::new(
            config.skip_prevout,
            config.skip_script_pubkey,
            receive_ordered_blocks,
            channels,
        );
        let fee_handle = thread::spawn(move || {
            fee.start();
        });

        read_handle.join().unwrap();
        parse_handle.join().unwrap();
        orderer_handle.join().unwrap();
        fee_handle.join().unwrap();
        info!("Total time elapsed: {}s", now.elapsed().as_secs());
    })
}

pub fn periodic_log_level(i: u32) -> Level {
    if i % 10_000 == 0 {
        Level::Info
    } else {
        Level::Debug
    }
}

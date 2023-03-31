use crate::utxo::{self, AnyUtxo};
use std::path::PathBuf;
use structopt::StructOpt;

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
    pub(crate) fn utxo_manager(&self) -> AnyUtxo {
        match &self.utxo_redb {
            Some(path) => AnyUtxo::Redb(utxo::RedbUtxo::new(path).unwrap()),
            None => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        }
    }
    #[cfg(feature = "db")]
    pub(crate) fn utxo_manager(&self) -> AnyUtxo {
        match (&self.utxo_db, &self.utxo_redb) {
            (Some(_), Some(_)) => panic!("utxo_db and utxo_redb cannot be specified together"),
            (Some(path), None) => AnyUtxo::Db(utxo::DbUtxo::new(path).unwrap()), //TODO unwrap
            (None, Some(path)) => AnyUtxo::Redb(utxo::RedbUtxo::new(path).unwrap()), //TODO unwra
            (None, None) => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        }
    }
}

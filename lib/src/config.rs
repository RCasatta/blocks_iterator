use bitcoin::Network;
#[cfg(feature = "clap")]
use clap::Parser;
use std::path::{Path, PathBuf};

/// Configuration parameters, most important the bitcoin blocks directory
#[cfg_attr(feature = "clap", derive(Parser))]
#[derive(Debug, Clone)]
pub struct Config {
    /// Blocks directory (containing `blocks*.dat`)
    #[cfg_attr(feature = "clap", arg(short, long))]
    pub blocks_dir: PathBuf,

    /// Network (bitcoin, testnet, regtest, signet)
    #[cfg_attr(feature = "clap", arg(short, long))]
    pub network: bitcoin::Network,

    /// Skip calculation of previous outputs, it's faster and it uses much less memory
    /// however make it impossible calculate fees or access tx input previous scripts
    #[cfg_attr(feature = "clap", arg(short, long))]
    pub skip_prevout: bool,

    /// Maximum length of a reorg allowed, during reordering send block to the next step only
    /// if it has `max_reorg` following blocks. Higher is more conservative, while lower faster.
    /// When parsing testnet blocks, it may be necessary to increase this a lot
    #[cfg_attr(feature = "clap", arg(short, long, default_value = "6"))]
    pub max_reorg: u8,

    /// Size of the channels used to pass messages between threads
    #[cfg_attr(feature = "clap", arg(short, long, default_value = "0"))]
    pub channels_size: u8,

    #[cfg(feature = "db")]
    /// Specify a **directory** where a rocks database will be created to store the Utxo (when `--skip-prevout` is not used)
    /// Reduce the memory requirements but it's slower and use disk space
    #[cfg_attr(feature = "clap", cfg_attr(feature = "clap", arg(short, long)))]
    pub utxo_db: Option<PathBuf>,

    #[cfg(feature = "redb")]
    /// Specify a **file** where a redb database will be created to store the Utxo (when `--skip-prevout` is not used)
    /// Reduce the memory requirements but it's slower and use disk space.
    ///
    /// Note with feature db you also have the options to use rocksdb, which is faster during creation of the utxo set
    /// but slower to compile.
    #[cfg_attr(feature = "clap", arg(long))]
    pub utxo_redb: Option<PathBuf>,

    /// Start the blocks iteration at the specified height, note blocks*.dat file are read and
    /// analyzed anyway to follow the blockchain starting at the genesis and populate utxos,
    /// however they are not emitted
    #[cfg_attr(feature = "clap", arg(long, default_value = "0"))]
    pub start_at_height: u32,

    /// Stop the blocks iteration at the specified height
    #[cfg_attr(feature = "clap", arg(long))]
    pub stop_at_height: Option<u32>,

    /// The serialization format to use for the generated `BlockExtra`
    #[cfg_attr(feature = "clap", arg(long, default_value = "1"))]
    pub serialization_version: u8,
}

impl Config {
    /// Creates a config with `path` and `network` and defaults parameters
    pub fn new<P: AsRef<Path>>(path: P, network: Network) -> Self {
        Self {
            blocks_dir: path.as_ref().to_owned(),
            network,
            skip_prevout: false,
            max_reorg: 6,
            channels_size: 0,
            #[cfg(feature = "db")]
            utxo_db: None,
            #[cfg(feature = "redb")]
            utxo_redb: None,
            start_at_height: 0,
            stop_at_height: None,
            serialization_version: 1,
        }
    }

    #[cfg(all(not(feature = "db"), not(feature = "redb")))]
    pub(crate) fn utxo_manager(&self) -> Result<crate::utxo::AnyUtxo, crate::Error> {
        use crate::utxo::{self, AnyUtxo};
        Ok(AnyUtxo::Mem(utxo::MemUtxo::new(self.network)))
    }

    #[cfg(all(not(feature = "db"), feature = "redb"))]
    pub(crate) fn utxo_manager(&self) -> Result<crate::utxo::AnyUtxo, crate::Error> {
        use crate::utxo::{self, AnyUtxo};
        Ok(match &self.utxo_redb {
            Some(path) => AnyUtxo::Redb(utxo::RedbUtxo::new(path)?),
            None => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        })
    }
    #[cfg(all(feature = "db", not(feature = "redb")))]
    pub(crate) fn utxo_manager(&self) -> Result<crate::utxo::AnyUtxo, crate::Error> {
        use crate::utxo::{self, AnyUtxo};
        Ok(match &self.utxo_db {
            Some(path) => AnyUtxo::Db(utxo::DbUtxo::new(path)?),
            None => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        })
    }
    #[cfg(all(feature = "db", feature = "redb"))]
    pub(crate) fn utxo_manager(&self) -> Result<crate::utxo::AnyUtxo, crate::Error> {
        use crate::utxo::{self, AnyUtxo};
        Ok(match (&self.utxo_db, &self.utxo_redb) {
            (Some(_), Some(_)) => return Err(crate::Error::OneDb),
            (Some(path), None) => AnyUtxo::Db(utxo::DbUtxo::new(path)?),
            (None, Some(path)) => AnyUtxo::Redb(utxo::RedbUtxo::new(path)?),
            (None, None) => AnyUtxo::Mem(utxo::MemUtxo::new(self.network)),
        })
    }
}

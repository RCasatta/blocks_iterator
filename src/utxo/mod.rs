use crate::bitcoin::{Block, TxOut};

mod mem;

#[cfg(feature = "db")]
mod db;

pub use mem::MemUtxo;

#[cfg(feature = "db")]
pub use db::DbUtxo;

pub trait Utxo {
    /// Add all the outputs of all the transaction in the block in the Utxo set
    fn add(&mut self, block: &Block, height: u32);

    /// Get all the prevouts in the block at `height` in the order they are found in the block.
    /// first element in the vector is the prevout of the first input of the first transaction after
    /// the coinbase
    fn get(&mut self, height: u32) -> Vec<TxOut>;

    /// return stats about the Utxo
    fn stat(&self) -> String;
}

pub enum AnyUtxo {
    #[cfg(feature = "db")]
    Db(db::DbUtxo),
    Mem(MemUtxo),
}

impl Utxo for AnyUtxo {
    fn add(&mut self, block: &Block, height: u32) {
        match self {
            #[cfg(feature = "db")]
            AnyUtxo::Db(db) => db.add(block, height),
            AnyUtxo::Mem(mem) => mem.add(block, height),
        }
    }

    fn get(&mut self, height: u32) -> Vec<TxOut> {
        match self {
            #[cfg(feature = "db")]
            AnyUtxo::Db(db) => db.get(height),
            AnyUtxo::Mem(mem) => mem.get(height),
        }
    }

    fn stat(&self) -> String {
        match self {
            #[cfg(feature = "db")]
            AnyUtxo::Db(db) => db.stat(),
            AnyUtxo::Mem(mem) => mem.stat(),
        }
    }
}

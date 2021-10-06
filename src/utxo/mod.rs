use crate::bitcoin::{Block, OutPoint, TxOut};

mod mem;

#[cfg(feature = "db")]
mod db;

pub use mem::MemUtxo;

#[cfg(feature = "db")]
pub use db::DbUtxo;

pub trait Utxo {
    /// Add all the outputs of all the transaction in the block in the Utxo set
    /// returns the Txid of each transaction in the order they are found in the block
    fn add(&mut self, block: &Block, height: u32);

    /// Remove and return an outpoint from the Utxo
    ///
    /// Some implementation (db) may decide to not physically remove the outpoint internally
    fn remove(&mut self, outpoint: &OutPoint) -> TxOut;

    /// Get all the prevouts in the block at `height` in reverse order they are found in the block.
    /// last element in the vector is the prevout of the first input of the first transaction after
    /// the coinbase
    ///
    /// Some implementation (memory) may return `None` and require to repeatedly call remove
    fn get_all(&self, height: u32) -> Option<Vec<TxOut>>;

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

    fn remove(&mut self, outpoint: &OutPoint) -> TxOut {
        match self {
            #[cfg(feature = "db")]
            AnyUtxo::Db(db) => db.remove(outpoint),
            AnyUtxo::Mem(mem) => mem.remove(outpoint),
        }
    }

    fn get_all(&self, height: u32) -> Option<Vec<TxOut>> {
        match self {
            #[cfg(feature = "db")]
            AnyUtxo::Db(db) => db.get_all(height),
            AnyUtxo::Mem(mem) => mem.get_all(height),
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

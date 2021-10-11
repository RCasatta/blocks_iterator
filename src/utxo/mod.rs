use crate::bitcoin::{Block, TxOut};

mod mem;

#[cfg(feature = "db")]
mod db;

pub use mem::MemUtxo;

use bitcoin::OutPoint;
#[cfg(feature = "db")]
pub use db::DbUtxo;

pub trait UtxoStore {
    /// Add all the outputs (except provably unspenof all the transaction in the block in the `UtxoStore`
    /// Return all the prevouts in the block at `height` in the order they are found in the block.
    /// First element in the vector is the prevout of the first input of the first transaction after
    /// the coinbase
    fn add_outputs_get_inputs(&mut self, block: &Block, height: u32) -> Vec<TxOut>;

    /// return stats about the Utxo
    fn stat(&self) -> String;
}

trait Hash64 {
    fn hash64(&self) -> u64;
}

trait Hash32 {
    fn hash32(&self) -> u32;
}

pub enum AnyUtxo {
    #[cfg(feature = "db")]
    Db(db::DbUtxo),
    Mem(MemUtxo),
}

impl UtxoStore for AnyUtxo {
    fn add_outputs_get_inputs(&mut self, block: &Block, height: u32) -> Vec<TxOut> {
        match self {
            #[cfg(feature = "db")]
            AnyUtxo::Db(db) => db.add_outputs_get_inputs(block, height),
            AnyUtxo::Mem(mem) => mem.add_outputs_get_inputs(block, height),
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

impl Hash64 for OutPoint {
    fn hash64(&self) -> u64 {
        fxhash::hash64(self)
    }
}

impl Hash32 for OutPoint {
    fn hash32(&self) -> u32 {
        fxhash::hash32(self)
    }
}

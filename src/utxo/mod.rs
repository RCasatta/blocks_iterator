use crate::bitcoin::{OutPoint, Transaction, TxOut, Txid};

mod mem;

pub use mem::MemUtxo;

pub trait Utxo {
    fn add(&mut self, tx: &Transaction) -> Txid;
    fn remove(&mut self, outpoint: OutPoint) -> TxOut;
    fn stat(&self) -> String;
}

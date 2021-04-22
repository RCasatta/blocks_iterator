use bitcoin::{TxOut, Txid};
use std::collections::HashMap;
use std::convert::TryInto;

type VecTxOut = Vec<Option<TxOut>>;

#[derive(Eq, PartialEq, Hash)]
struct TruncatedHash([u8; 5]);

/// A map like struct storing truncated keys to save memory, in case of collisions a fallback map
/// with the full key is used
pub struct TruncMap {
    trunc: HashMap<TruncatedHash, VecTxOut>,
    full: HashMap<Txid, VecTxOut>,
}

impl From<&Txid> for TruncatedHash {
    fn from(txid: &Txid) -> Self {
        TruncatedHash((&txid[0..5]).try_into().unwrap())
    }
}

impl Default for TruncMap {
    fn default() -> Self {
        TruncMap {
            trunc: HashMap::new(),
            full: HashMap::new(),
        }
    }
}

impl TruncMap {
    pub fn insert(&mut self, txid: Txid, value: VecTxOut) -> Option<VecTxOut> {
        let truncated_txid = (&txid).into();
        if self.trunc.get(&truncated_txid).is_some() {
            self.full.insert(txid, value)
        } else {
            self.trunc.insert(truncated_txid, value)
        }
    }

    pub fn remove(&mut self, txid: &Txid) -> Option<VecTxOut> {
        if let Some(val) = self.full.remove(txid) {
            Some(val)
        } else {
            self.trunc.remove(&txid.into())
        }
    }

    pub fn len(&self) -> (usize, usize) {
        (self.trunc.len(), self.full.len())
    }
}

use bitcoin::{OutPoint, TxOut};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Eq, PartialEq, Hash)]
struct TruncatedKey(u64);

/// A map like struct storing truncated keys to save memory, in case of collisions a fallback map
/// with the full key is used
/// It obviously loose the ability to iterate over keys
pub struct TruncMap {
    trunc: HashMap<TruncatedKey, TxOut>,
    full: HashMap<OutPoint, TxOut>,
}

impl From<&OutPoint> for TruncatedKey {
    fn from(outpoint: &OutPoint) -> Self {
        let mut hasher = DefaultHasher::new();
        outpoint.hash(&mut hasher);
        TruncatedKey(hasher.finish())
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
    pub fn insert(&mut self, outpoint: OutPoint, value: TxOut) -> Option<TxOut> {
        let truncated_outpoint = (&outpoint).into();
        if self.trunc.get(&truncated_outpoint).is_some() {
            self.full.insert(outpoint, value)
        } else {
            self.trunc.insert(truncated_outpoint, value)
        }
    }

    pub fn remove(&mut self, outpoint: &OutPoint) -> Option<TxOut> {
        if let Some(val) = self.full.remove(outpoint) {
            Some(val)
        } else {
            self.trunc.remove(&outpoint.into())
        }
    }

    pub fn len(&self) -> (usize, usize) {
        (self.trunc.len(), self.full.len())
    }
}

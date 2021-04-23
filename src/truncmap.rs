use bitcoin::{OutPoint, TxOut};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};

#[derive(Eq, PartialEq, Hash)]
struct TruncatedKey(u64);

/// A map like struct storing truncated keys to save memory, in case of collisions a fallback map
/// with the full key is used.
/// It obviously loose the ability to iterate over keys
pub struct TruncMap {
    /// use a PassthroughHasher since `From<&Outpoint>` it's already hashing the key
    trunc: HashMap<TruncatedKey, TxOut, PassthroughHasher>,
    full: HashMap<OutPoint, TxOut>,
}

impl From<&OutPoint> for TruncatedKey {
    fn from(outpoint: &OutPoint) -> Self {
        let mut hasher = DefaultHasher::new();
        outpoint.hash(&mut hasher);
        TruncatedKey(hasher.finish())
    }
}

impl TruncMap {
    pub fn insert(&mut self, outpoint: OutPoint, value: &TxOut) {
        // we optimistically insert since collision must be rare
        let old = self.trunc.insert((&outpoint).into(), value.clone());

        if let Some(old) = old {
            // rolling back since the element did exist
            self.trunc.insert((&outpoint).into(), old);
            // since key collided, saving in the full map
            self.full.insert(outpoint, value.clone());
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

impl Default for TruncMap {
    fn default() -> Self {
        TruncMap {
            trunc: HashMap::<TruncatedKey, TxOut, PassthroughHasher>::with_hasher(
                PassthroughHasher::default(),
            ),
            full: HashMap::new(),
        }
    }
}

struct PassthroughHasher(u64);

impl Default for PassthroughHasher {
    fn default() -> Self {
        PassthroughHasher(0)
    }
}

impl BuildHasher for PassthroughHasher {
    type Hasher = PassthroughHasher;

    fn build_hasher(&self) -> Self::Hasher {
        PassthroughHasher(0)
    }
}

impl Hasher for PassthroughHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _bytes: &[u8]) {
        todo!("passtrough hasher should not pass here!")
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

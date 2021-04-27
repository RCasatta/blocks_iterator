use bitcoin::{OutPoint, TxOut};
use fxhash::FxHashMap;
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};

/// A map like struct storing truncated keys to save memory, in case of collisions a fallback map
/// with the full key is used. This is only possible because we know OutPoint are unique.
/// It obviously loose the ability to iterate over keys
pub struct TruncMap {
    /// use a PassthroughHasher since the key it's already an hash
    trunc: HashMap<u64, TxOut, PassthroughHasher>,
    full: FxHashMap<OutPoint, TxOut>,
    build_hasher: fxhash::FxBuildHasher,
}

impl TruncMap {
    /// insert a value in the map
    /// value is Cow<>, because in the more common case if I would accept TxOut but the caller has &TxOut 2 clones in total would be necessary (1 from the caller and 1 inside) while with the Cow only 1 is needed
    /// when accepting &TxOut but the caller has TxOut, we internally need 1 clone in both cases
    pub fn insert(&mut self, outpoint: OutPoint, value: Cow<TxOut>) {
        // we optimistically insert since collision must be rare
        let old = self
            .trunc
            .insert(self.hash(&outpoint), value.clone().into_owned());

        if let Some(old) = old {
            // rolling back since the element did exist
            self.trunc.insert(self.hash(&outpoint), old);
            // since key collided, saving in the full map
            self.full.insert(outpoint, value.into_owned());
        }
    }

    pub fn remove(&mut self, outpoint: &OutPoint) -> Option<TxOut> {
        if let Some(val) = self.full.remove(outpoint) {
            Some(val)
        } else {
            self.trunc.remove(&self.hash(outpoint))
        }
    }

    pub fn len(&self) -> (usize, usize) {
        (self.trunc.len(), self.full.len())
    }

    fn hash(&self, outpoint: &OutPoint) -> u64 {
        let mut hasher = self.build_hasher.build_hasher();
        outpoint.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for TruncMap {
    fn default() -> Self {
        TruncMap {
            trunc: HashMap::<u64, TxOut, PassthroughHasher>::with_hasher(
                PassthroughHasher::default(),
            ),
            full: FxHashMap::default(),
            build_hasher: Default::default(),
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
        panic!("passtrough hasher should not pass here!")
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

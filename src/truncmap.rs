use bitcoin::{OutPoint, Script, TxOut};
use fxhash::FxHashMap;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};

/// A map like struct storing truncated keys to save memory, in case of collisions a fallback map
/// with the full key is used.
/// It obviously loose the ability to iterate over keys
pub struct TruncMap {
    /// use a PassthroughHasher since the key it's already an hash
    trunc: HashMap<u64, ScriptValue, PassthroughHasher>,
    full: FxHashMap<OutPoint, TxOut>,
    scripts: FxHashMap<u64, (Script, u64)>,
    build_hasher: fxhash::FxBuildHasher,
}

impl TruncMap {
    pub fn insert(&mut self, outpoint: OutPoint, tx_out: &TxOut) {
        let script_hash = if tx_out.script_pubkey != Script::default() {
            let script_hash = self.hash_script(&tx_out.script_pubkey);
            self.scripts
                .entry(script_hash)
                .and_modify(|c| c.1 += 1)
                .or_insert((tx_out.script_pubkey.clone(), 1));
            script_hash
        } else {
            0
        };
        let script_value = ScriptValue {
            script_ref: script_hash,
            value: tx_out.value,
        };
        // we optimistically insert since collision must be rare
        let old = self.trunc.insert(self.hash(&outpoint), script_value);

        if let Some(old) = old {
            // rolling back since the element did exist
            self.trunc.insert(self.hash(&outpoint), old);
            // since key collided, saving in the full map
            self.full.insert(outpoint, tx_out.clone());
        }
    }

    pub fn remove(&mut self, outpoint: &OutPoint) -> Option<TxOut> {
        if let Some(val) = self.full.remove(outpoint) {
            Some(val)
        } else {
            let sv = self.trunc.remove(&self.hash(outpoint)).unwrap();
            let (script, counter) = self.scripts.remove(&sv.script_ref).unwrap_or_default();
            if counter > 1 {
                self.scripts
                    .insert(sv.script_ref, (script.clone(), counter - 1));
            }
            Some(TxOut {
                script_pubkey: script,
                value: sv.value,
            })
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
    fn hash_script(&self, script: &Script) -> u64 {
        let mut hasher = self.build_hasher.build_hasher();
        script.hash(&mut hasher);
        hasher.finish()
    }
}

struct ScriptValue {
    script_ref: u64,
    value: u64,
}

impl Default for TruncMap {
    fn default() -> Self {
        TruncMap {
            trunc: HashMap::<u64, ScriptValue, PassthroughHasher>::with_hasher(
                PassthroughHasher::default(),
            ),
            scripts: FxHashMap::default(),
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

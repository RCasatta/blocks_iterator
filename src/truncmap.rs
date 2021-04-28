use bitcoin::{OutPoint, PubkeyHash, Script, ScriptHash, TxOut, WPubkeyHash};
use fxhash::FxHashMap;
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};

/// A map like struct storing truncated keys to save memory, in case of collisions a fallback map
/// with the full key is used. This is only possible because we know OutPoint are unique.
/// It obviously loose the ability to iterate over keys
pub struct TruncMap {
    /// use a PassthroughHasher since the key it's already an hash
    trunc: HashMap<u64, (StackScript, u64), PassthroughHasher>,
    full: FxHashMap<OutPoint, TxOut>,
    build_hasher: fxhash::FxBuildHasher,
    script_stack: u64,
    script_other: u64,
}

/// A 24 bytes struct to store most of the script in the blockchain on the stack
enum StackScript {
    //P2Pk(PublicKey),     // with this sizeof would grow to 72
    //V0Wsh(WScriptHash),  // with this sizeof would grow to 40
    P2Pkh(PubkeyHash),
    P2Sh(ScriptHash),
    V0Wpkh(WPubkeyHash),
    Other(Script),
}

impl StackScript {
    pub fn is_other(&self) -> bool {
        match self {
            StackScript::Other(_) => true,
            _ => false,
        }
    }
}

impl From<Script> for StackScript {
    fn from(script: Script) -> Self {
        //TODO populate with right hash values
        if script.is_p2pkh() {
            StackScript::P2Pkh(PubkeyHash::default())
        } else if script.is_p2sh() {
            StackScript::P2Sh(ScriptHash::default())
        } else if script.is_v0_p2wpkh() {
            StackScript::V0Wpkh(WPubkeyHash::default())
        } else {
            StackScript::Other(script)
        }
    }
}

impl From<StackScript> for Script {
    fn from(stack_script: StackScript) -> Self {
        match stack_script {
            StackScript::Other(script) => script,
            StackScript::P2Pkh(h) => Script::new_p2pkh(&h),
            StackScript::P2Sh(h) => Script::new_p2sh(&h),
            StackScript::V0Wpkh(h) => Script::new_v0_wpkh(&h),
        }
    }
}

impl TruncMap {
    /// insert a value in the map
    /// value is Cow<>, because in the more common case if I would accept TxOut but the caller has &TxOut 2 clones in total would be necessary (1 from the caller and 1 inside) while with the Cow only 1 is needed
    /// when accepting &TxOut but the caller has TxOut, we internally need 1 clone in both cases
    pub fn insert(&mut self, outpoint: OutPoint, value: Cow<TxOut>) {
        let tx_out = value.clone().into_owned();
        let tx_out_stack: (StackScript, u64) = (tx_out.script_pubkey.into(), tx_out.value);
        if tx_out_stack.0.is_other() {
            self.script_other += 1;
        } else {
            self.script_stack += 1;
        }

        // we optimistically insert since collision must be rare
        let old = self.trunc.insert(self.hash(&outpoint), tx_out_stack);

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
            self.trunc.remove(&self.hash(outpoint)).map(|val| TxOut {
                script_pubkey: val.0.into(),
                value: val.1,
            })
        }
    }

    pub fn len(&self) -> (usize, usize, usize) {
        (self.trunc.len(), self.full.len(), self.trunc.capacity())
    }

    pub fn script_on_stack(&self) -> f64 {
        self.script_stack as f64 / ((self.script_other + self.script_stack) as f64)
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
            trunc: HashMap::<u64, (StackScript, u64), PassthroughHasher>::with_hasher(
                PassthroughHasher::default(),
            ),
            full: FxHashMap::default(),
            build_hasher: Default::default(),
            script_other: 0,
            script_stack: 0,
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

#[cfg(test)]
mod test {
    use crate::truncmap::StackScript;
    use bitcoin::{PubkeyHash, PublicKey, ScriptHash, TxOut, WPubkeyHash, WScriptHash};

    #[test]
    fn test_size() {
        assert_eq!(std::mem::size_of::<StackScript>(), 24);
        assert_eq!(std::mem::size_of::<PublicKey>(), 65);
        assert_eq!(std::mem::size_of::<PubkeyHash>(), 20);
        assert_eq!(std::mem::size_of::<ScriptHash>(), 20);
        assert_eq!(std::mem::size_of::<WPubkeyHash>(), 20);
        assert_eq!(std::mem::size_of::<WScriptHash>(), 32);
        assert_eq!(std::mem::size_of::<Box<[u8]>>(), 16);
        assert_eq!(std::mem::size_of::<(StackScript, u64)>(), 32);
    }
}

use crate::bitcoin::{Block, Transaction, Txid};
use crate::utxo::Utxo;
use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, PubkeyHash, Script, ScriptHash, TxOut, WPubkeyHash};
use fxhash::FxHashMap;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};

pub struct MemUtxo {
    map: TruncMap,
    unspendable: u64,
    block_prevouts: HashMap<u32, Vec<TxOut>>,
}

impl MemUtxo {
    pub fn new() -> Self {
        MemUtxo {
            map: TruncMap::default(),
            unspendable: 0,
            block_prevouts: HashMap::new(),
        }
    }
}

impl MemUtxo {
    fn add_tx_outputs(&mut self, tx: &Transaction) -> Txid {
        let txid = tx.txid();
        for (i, output) in tx.output.iter().enumerate() {
            if output.script_pubkey.is_provably_unspendable() {
                self.unspendable += 1;
                continue;
            }
            self.map.insert(OutPoint::new(txid, i as u32), output);
        }
        txid
    }
}

impl Utxo for MemUtxo {
    fn add(&mut self, block: &Block, height: u32) {
        for tx in block.txdata.iter() {
            self.add_tx_outputs(tx);
        }
        let total_inputs = block.txdata.iter().skip(1).map(|e| e.input.len()).sum();
        let mut prevouts = Vec::with_capacity(total_inputs);
        for tx in block.txdata.iter().skip(1) {
            for input in tx.input.iter() {
                let tx_out = self.map.remove(&input.previous_output).unwrap();
                prevouts.push(tx_out);
            }
        }
        self.block_prevouts.insert(height, prevouts);
    }

    fn get(&mut self, height: u32) -> Vec<TxOut> {
        self.block_prevouts.remove(&height).unwrap()
    }

    fn stat(&self) -> String {
        let utxo_size = self.map.trunc.len();
        let collision_size = self.map.full.len();
        let utxo_capacity = self.map.trunc.capacity();
        let script_on_stack = (self.map.script_stack as f64
            / ((self.map.script_other + self.map.script_stack) as f64))
            * 100.0;
        let unspendable = self.unspendable;
        let load = (utxo_size as f64 / utxo_capacity as f64) * 100.0;

        format!(
            "(utxo, collision, capacity): {:?} load:{:.1}% script on stack: {:.1}% unspendable:{}",
            (utxo_size, collision_size, utxo_capacity),
            load,
            script_on_stack,
            unspendable
        )
    }
}

/// A map like struct storing truncated keys to save memory, in case of collisions a fallback map
/// with the full key is used. This is only possible because we know OutPoints are unique.
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
#[derive(Debug, Eq, PartialEq)]
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

impl From<&Script> for StackScript {
    fn from(script: &Script) -> Self {
        if script.is_p2pkh() {
            StackScript::P2Pkh(PubkeyHash::from_slice(&script[3..23]).unwrap())
        } else if script.is_p2sh() {
            StackScript::P2Sh(ScriptHash::from_slice(&script[2..22]).unwrap())
        } else if script.is_v0_p2wpkh() {
            StackScript::V0Wpkh(WPubkeyHash::from_slice(&script[2..22]).unwrap())
        } else {
            StackScript::Other(script.clone())
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
    pub fn insert(&mut self, outpoint: OutPoint, tx_out: &TxOut) {
        let tx_out_stack: (StackScript, u64) = ((&tx_out.script_pubkey).into(), tx_out.value);
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
            self.full.insert(outpoint, tx_out.clone());
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

    fn hash(&self, outpoint: &OutPoint) -> u64 {
        let mut hasher = self.build_hasher.build_hasher();
        std::hash::Hash::hash(outpoint, &mut hasher);
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
    use crate::utxo::mem::StackScript;
    use crate::FsBlock;
    use bitcoin::hashes::Hash;
    use bitcoin::{PubkeyHash, PublicKey, Script, ScriptHash, WPubkeyHash, WScriptHash};

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
        assert_eq!(std::mem::size_of::<FsBlock>(), 112);
    }

    #[test]
    fn test_script_stack() {
        let hash = PubkeyHash::from_slice(&[9u8; 20]).unwrap();
        let script = Script::new_p2pkh(&hash);
        let stack_script: StackScript = (&script).into();
        assert_eq!(stack_script, StackScript::P2Pkh(hash));

        let hash = ScriptHash::from_slice(&[8u8; 20]).unwrap();
        let script = Script::new_p2sh(&hash);
        let stack_script: StackScript = (&script).into();
        assert_eq!(stack_script, StackScript::P2Sh(hash));

        let hash = WPubkeyHash::from_slice(&[7u8; 20]).unwrap();
        let script = Script::new_v0_wpkh(&hash);
        let stack_script: StackScript = (&script).into();
        assert_eq!(stack_script, StackScript::V0Wpkh(hash));
    }
}

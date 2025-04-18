use crate::bitcoin::{Network, Transaction, Txid};
use crate::utxo::{Hash64, UtxoStore};
use crate::BlockExtra;
use bitcoin::hashes::Hash;
use bitcoin::{Amount, OutPoint, PubkeyHash, ScriptBuf, ScriptHash, TxOut, WPubkeyHash};
use fxhash::FxHashMap;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};

pub struct MemUtxo {
    map: TruncMap,
    unspendable: u64,
}

impl MemUtxo {
    pub fn new(network: Network) -> Self {
        MemUtxo {
            map: TruncMap::new(network),
            unspendable: 0,
        }
    }
}

impl MemUtxo {
    fn add_tx_outputs(&mut self, txid: &Txid, tx: &Transaction) {
        for (i, output) in tx.output.iter().enumerate() {
            if output.script_pubkey.is_op_return() {
                self.unspendable += 1;
                continue;
            }
            self.map.insert(OutPoint::new(*txid, i as u32), output);
        }
    }
}

impl UtxoStore for MemUtxo {
    fn add_outputs_get_inputs(&mut self, block_extra: &BlockExtra, _height: u32) -> Vec<TxOut> {
        let block = block_extra.block();
        for (txid, tx) in block_extra.iter_tx() {
            self.add_tx_outputs(txid, tx);
        }
        let mut prevouts = Vec::with_capacity(block_extra.block_total_inputs());
        for tx in block.txdata.iter().skip(1) {
            for input in tx.input.iter() {
                let tx_out = self.map.remove(&input.previous_output).unwrap();
                prevouts.push(tx_out);
            }
        }
        prevouts
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
    P2V0Wpkh(WPubkeyHash),
    Other(ScriptBuf),
}

impl StackScript {
    pub fn is_other(&self) -> bool {
        match self {
            StackScript::Other(_) => true,
            _ => false,
        }
    }
}

impl From<&ScriptBuf> for StackScript {
    fn from(script: &ScriptBuf) -> Self {
        if script.is_p2pkh() {
            StackScript::P2Pkh(PubkeyHash::from_slice(&script.as_bytes()[3..23]).unwrap())
        } else if script.is_p2sh() {
            StackScript::P2Sh(ScriptHash::from_slice(&script.as_bytes()[2..22]).unwrap())
        } else if script.is_p2wpkh() {
            StackScript::P2V0Wpkh(WPubkeyHash::from_slice(&script.as_bytes()[2..22]).unwrap())
        } else {
            StackScript::Other(script.clone())
        }
    }
}

impl From<StackScript> for ScriptBuf {
    fn from(stack_script: StackScript) -> Self {
        match stack_script {
            StackScript::Other(script) => script,
            StackScript::P2Pkh(h) => ScriptBuf::new_p2pkh(&h),
            StackScript::P2Sh(h) => ScriptBuf::new_p2sh(&h),
            StackScript::P2V0Wpkh(h) => ScriptBuf::new_p2wpkh(&h),
        }
    }
}

impl TruncMap {
    /// insert a value in the map
    pub fn insert(&mut self, outpoint: OutPoint, tx_out: &TxOut) {
        let tx_out_stack: (StackScript, u64) =
            ((&tx_out.script_pubkey).into(), tx_out.value.to_sat());
        if tx_out_stack.0.is_other() {
            self.script_other += 1;
        } else {
            self.script_stack += 1;
        }

        // we optimistically insert since collision must be rare
        let old = self.trunc.insert(outpoint.hash64(), tx_out_stack);

        if let Some(old) = old {
            // rolling back since the element did exist
            self.trunc.insert(outpoint.hash64(), old);
            // since key collided, saving in the full map
            self.full.insert(outpoint, tx_out.clone());
        }
    }

    pub fn remove(&mut self, outpoint: &OutPoint) -> Option<TxOut> {
        if let Some(val) = self.full.remove(outpoint) {
            Some(val)
        } else {
            self.trunc.remove(&outpoint.hash64()).map(|val| TxOut {
                script_pubkey: val.0.into(),
                value: Amount::from_sat(val.1),
            })
        }
    }
}

impl TruncMap {
    fn new(network: Network) -> Self {
        // to avoid re-allocation and re-hashing of the map we use some known capacity needed
        // at given height
        let capacity = match network {
            Network::Bitcoin => 98_959_418, // @704065 load:76.1%
            Network::Testnet => 28_038_982, // @2097712 load:93.2%
            Network::Signet => 1 >> 20,
            Network::Regtest => 1 >> 10,
            _ => panic!("unrecognized network"),
        };

        TruncMap {
            trunc: HashMap::<u64, (StackScript, u64), PassthroughHasher>::with_capacity_and_hasher(
                capacity,
                PassthroughHasher::default(),
            ),
            full: FxHashMap::default(),
            script_other: 0,
            script_stack: 0,
        }
    }
}

#[derive(Default)]
struct PassthroughHasher(u64);

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
    use bitcoin::{PubkeyHash, PublicKey, ScriptBuf, ScriptHash, WPubkeyHash, WScriptHash};

    #[test]
    fn test_size() {
        assert_eq!(std::mem::size_of::<StackScript>(), 32);
        assert_eq!(std::mem::size_of::<PublicKey>(), 65);
        assert_eq!(std::mem::size_of::<PubkeyHash>(), 20);
        assert_eq!(std::mem::size_of::<ScriptHash>(), 20);
        assert_eq!(std::mem::size_of::<WPubkeyHash>(), 20);
        assert_eq!(std::mem::size_of::<WScriptHash>(), 32);
        assert_eq!(std::mem::size_of::<Box<[u8]>>(), 16);
        assert_eq!(std::mem::size_of::<(StackScript, u64)>(), 40);
        assert_eq!(std::mem::size_of::<FsBlock>(), 128);
    }

    #[test]
    fn test_script_stack() {
        let hash = PubkeyHash::from_slice(&[9u8; 20]).unwrap();
        let script = ScriptBuf::new_p2pkh(&hash);
        let stack_script: StackScript = (&script).into();
        assert_eq!(stack_script, StackScript::P2Pkh(hash));

        let hash = ScriptHash::from_slice(&[8u8; 20]).unwrap();
        let script = ScriptBuf::new_p2sh(&hash);
        let stack_script: StackScript = (&script).into();
        assert_eq!(stack_script, StackScript::P2Sh(hash));

        let hash = WPubkeyHash::from_slice(&[7u8; 20]).unwrap();
        let script = ScriptBuf::new_p2wpkh(&hash);
        let stack_script: StackScript = (&script).into();
        assert_eq!(stack_script, StackScript::P2V0Wpkh(hash));
    }
}

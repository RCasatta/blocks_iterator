use crate::bitcoin::consensus::serialize;
use crate::bitcoin::hashes::{sha256, Hash, HashEngine};
use crate::bitcoin::{Block, OutPoint, TxOut};
use crate::utxo::UtxoStore;
use bitcoin::consensus::deserialize;
use log::{debug, info};
use rand::Rng;
use rocksdb::{DBCompressionType, Options, WriteBatch, DB};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;

type Key = [u8; 12];

pub struct DbUtxo {
    db: DB,
    updated_up_to_height: i32,
    inserted_outputs: u64,
    salt: Key,
}

const SALT: &'static str = "salt";
const UPDATED_UP_TO_HEIGHT: &'static str = "updated_up_to_height";

impl DbUtxo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<DbUtxo, rocksdb::Error> {
        let mut options = Options::default();
        options.set_compression_type(DBCompressionType::Snappy);
        options.create_if_missing(true);
        let db = DB::open(&options, path)?;

        let updated_up_to_height = db
            .get(UPDATED_UP_TO_HEIGHT)?
            .map(|e| e.try_into().unwrap())
            .map(|e| i32::from_ne_bytes(e))
            .unwrap_or(-1);

        let salt: Option<Key> = db.get(SALT)?.map(|e| e.try_into().unwrap());
        let salt = match salt {
            Some(salt) => salt,
            None => {
                let salt = rand::thread_rng().gen::<Key>();
                db.put(SALT, &salt)?;
                salt
            }
        };
        info!(
            "using salt: {:?} updated_height: {}",
            salt, updated_up_to_height
        );

        Ok(DbUtxo {
            db,
            updated_up_to_height,
            inserted_outputs: 0,
            salt,
        })
    }
}

impl UtxoStore for DbUtxo {
    fn add_outputs_get_inputs(&mut self, block: &Block, height: u32) -> Vec<TxOut> {
        let height = height as i32;
        debug!(
            "height: {} updated_up_to: {}",
            height, self.updated_up_to_height
        );
        if height > self.updated_up_to_height {
            // since we can spend outputs created in this same block, we first put outputs in memory...
            let total_outputs = block.txdata.iter().map(|e| e.output.len()).sum();
            let mut block_outputs = HashMap::with_capacity(total_outputs);
            for tx in block.txdata.iter() {
                let txid = tx.txid();
                for (i, output) in tx.output.iter().enumerate() {
                    if !output.script_pubkey.is_provably_unspendable() {
                        let outpoint = OutPoint::new(txid, i as u32);
                        block_outputs.insert(outpoint, output);
                    }
                }
            }

            let total_inputs = block.txdata.iter().skip(1).map(|e| e.input.len()).sum();
            let mut prevouts = Vec::with_capacity(total_inputs);
            let mut batch = WriteBatch::default();
            for tx in block.txdata.iter().skip(1) {
                for input in tx.input.iter() {
                    //...then we first check if inputs spend output created in this block
                    match block_outputs.remove(&input.previous_output) {
                        Some(tx_out) => {
                            // we avoid touching the db entirely if it's spent in the same block
                            prevouts.push(tx_out.clone())
                        }
                        None => {
                            let key = input.previous_output.to_key(&self.salt);
                            let tx_out = deserialize(&self.db.get(&key).unwrap().unwrap()).unwrap();
                            batch.delete(&key);
                            prevouts.push(tx_out);
                        }
                    }
                }
            }

            // and we put all the remaining outputs in db
            for (k, v) in block_outputs.drain() {
                batch.put(&k.to_key(&self.salt), serialize(v));
                self.inserted_outputs += 1;
            }
            batch.put(height.to_ne_bytes(), serialize(&prevouts));
            batch.put(UPDATED_UP_TO_HEIGHT, height.to_ne_bytes());
            self.db.write(batch).unwrap(); // TODO unwrap
        }
        self.db
            .get(height.to_ne_bytes())
            .unwrap()
            .map(|e| deserialize(&e).unwrap())
            .unwrap()
    }

    fn stat(&self) -> String {
        format!(
            "updated_up_to_height: {} inserted_outputs: {}",
            self.updated_up_to_height, self.inserted_outputs
        )
    }
}

trait ToKey<T: AsRef<[u8]>> {
    fn to_key(&self, salt: &T) -> T;
}

impl ToKey<Key> for OutPoint {
    fn to_key(&self, salt: &Key) -> Key {
        let mut engine = sha256::HashEngine::default();
        engine.input(&salt[..]);
        engine.input(&self.txid.as_ref());
        engine.input(&self.vout.to_ne_bytes()[..]);
        let hash = sha256::Hash::from_engine(engine);
        let mut result = [0u8; 12];
        result.copy_from_slice(&hash.into_inner()[..12]);
        result
    }
}

#[cfg(test)]
mod test {
    use crate::utxo::db::ToKey;
    use bitcoin::OutPoint;

    #[test]
    fn test_key_hash() {
        let mut outpoint = OutPoint::default();
        outpoint.vout = 0;
        let salt = [1u8; 12];
        let before = outpoint.to_key(&salt);
        outpoint.vout = 1;
        assert_ne!(&outpoint.to_key(&salt), &before);
    }
}

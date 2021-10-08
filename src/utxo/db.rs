use crate::bitcoin::consensus::serialize;
use crate::bitcoin::{Block, OutPoint, TxOut};
use crate::utxo::{Hash32, Hash64, Utxo};
use bitcoin::consensus::deserialize;
use log::debug;
use rocksdb::{DBCompressionType, Options, WriteBatch, DB};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;

pub struct DbUtxo {
    db: DB,
    updated_up_to_height: i32,
    inserted_outputs: u64,
}

impl DbUtxo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<DbUtxo, rocksdb::Error> {
        let mut options = Options::default();
        options.set_compression_type(DBCompressionType::Snappy);
        options.create_if_missing(true);
        let db = DB::open(&options, path)?;

        let updated_up_to_height = db
            .get("updated_up_to_height")?
            .map(|e| e.try_into().unwrap())
            .map(|e| i32::from_ne_bytes(e))
            .unwrap_or(-1);

        Ok(DbUtxo {
            db,
            updated_up_to_height,
            inserted_outputs: 0,
        })
    }
}

impl Utxo for DbUtxo {
    fn add(&mut self, block: &Block, height: u32) {
        let height = height as i32;
        debug!(
            "height: {} updated_up_to: {}",
            height, self.updated_up_to_height
        );
        if height > self.updated_up_to_height {
            // since we can spend outputs created in this same block, we first put outputs in memory...
            let total_outputs = block.txdata.iter().map(|e| e.output.len()).sum();
            let mut current_outputs = HashMap::with_capacity(total_outputs);
            for tx in block.txdata.iter() {
                let txid = tx.txid();
                for (i, output) in tx.output.iter().enumerate() {
                    let outpoint = OutPoint::new(txid, i as u32);
                    current_outputs.insert(outpoint, output);
                }
            }

            let total_inputs = block.txdata.iter().skip(1).map(|e| e.input.len()).sum();
            let mut prevouts = Vec::with_capacity(total_inputs);
            let mut batch = WriteBatch::default();
            for tx in block.txdata.iter().skip(1) {
                for input in tx.input.iter() {
                    //...then we first check if inputs spend output created in this block
                    match current_outputs.remove(&input.previous_output) {
                        Some(tx_out) => {
                            // we avoid touching the db entirely if it's spent in the same block
                            prevouts.push(tx_out.clone())
                        }
                        None => {
                            let key = input.previous_output.to_key();
                            let tx_out = deserialize(&self.db.get(&key).unwrap().unwrap()).unwrap();
                            batch.delete(&key);
                            prevouts.push(tx_out);
                            self.inserted_outputs += 1;
                        }
                    }
                }
            }

            // and we put all the remaining outputs in db
            for (k, v) in current_outputs.drain() {
                batch.put(&k.to_key(), serialize(v));
            }
            batch.put(height.to_ne_bytes(), serialize(&prevouts));
            batch.put("updated_up_to_height", height.to_ne_bytes());
            self.db.write(batch).unwrap(); // TODO unwrap
        }
    }

    fn get(&mut self, height: u32) -> Vec<TxOut> {
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
    fn to_key(&self) -> T;
}

impl ToKey<[u8; 12]> for OutPoint {
    fn to_key(&self) -> [u8; 12] {
        let h64 = self.hash64().to_ne_bytes();
        let h32 = self.hash32().to_ne_bytes();
        let mut result = [0u8; 12];
        result[..8].copy_from_slice(&h64);
        result[8..].copy_from_slice(&h32);
        result
    }
}

#[cfg(test)]
mod test {
    use rocksdb::DB;

    #[test]
    fn test_rocks() {
        let db = DB::open_default("rocks").unwrap();

        for i in 0i32..10_000_000 {
            db.put(&format!("key{}", i), &i.to_ne_bytes()).unwrap();
        }
    }
}

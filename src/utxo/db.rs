use crate::bitcoin::consensus::serialize;
use crate::bitcoin::{Block, OutPoint, TxOut};
use crate::utxo::Utxo;
use bitcoin::consensus::deserialize;
use log::debug;
use rocksdb::DB;
use std::convert::TryInto;
use std::path::Path;

pub struct DbUtxo {
    db: DB,
    updated_up_to_height: i32,
}

impl DbUtxo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<DbUtxo, rocksdb::Error> {
        let db = DB::open_default(path)?;
        let updated_up_to_height = db
            .get("updated_up_to_height")?
            .map(|e| e.try_into().unwrap())
            .map(|e| i32::from_be_bytes(e))
            .unwrap_or(-1);

        Ok(DbUtxo {
            db,
            updated_up_to_height,
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
        if height >= self.updated_up_to_height {
            let mut txids = Vec::with_capacity(block.txdata.len());
            let mut prevouts = Vec::with_capacity(block.txdata.iter().map(|e| e.input.len()).sum());
            for tx in block.txdata.iter() {
                let txid = tx.txid();
                for (i, output) in tx.output.iter().enumerate() {
                    let outpoint = OutPoint::new(txid, i as u32);
                    self.db
                        .put(serialize(&outpoint), serialize(output))
                        .unwrap(); //TODO use batch, remove unwrap
                }
                txids.push(txid);

                if !tx.is_coin_base() {
                    for input in tx.input.iter() {
                        let tx_out = self.remove(&input.previous_output);
                        prevouts.push(tx_out);
                    }
                }
            }
            prevouts.reverse();
            self.db
                .put(height.to_be_bytes(), serialize(&prevouts))
                .unwrap();
            self.db
                .put("updated_up_to_height", height.to_be_bytes())
                .unwrap();
        }
    }

    fn remove(&mut self, outpoint: &OutPoint) -> TxOut {
        deserialize(&self.db.get(serialize(&outpoint)).unwrap().unwrap()).unwrap()
    }

    fn get_all(&self, height: u32) -> Option<Vec<TxOut>> {
        self.db
            .get(height.to_be_bytes())
            .unwrap()
            .map(|e| deserialize(&e).unwrap())
    }

    fn stat(&self) -> String {
        "".to_string()
    }
}

#[cfg(test)]
mod test {
    use rocksdb::DB;

    #[test]
    fn test_rocks() {
        let db = DB::open_default("rocks").unwrap();

        for i in 0i32..10_000_000 {
            db.put(&format!("key{}", i), &i.to_be_bytes()).unwrap();
        }
    }
}

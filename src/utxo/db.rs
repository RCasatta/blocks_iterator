use crate::bitcoin::consensus::serialize;
use crate::bitcoin::{Block, OutPoint, TxOut, Txid};
use crate::utxo::Utxo;
use bitcoin::consensus::deserialize;
use rocksdb::DB;
use std::path::Path;

pub struct DbUtxo {
    db: DB,
}

impl DbUtxo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<DbUtxo, rocksdb::Error> {
        let db = DB::open_default(path)?;
        //TODO verify updated at
        Ok(DbUtxo { db })
    }
}

impl Utxo for DbUtxo {
    fn add(&mut self, block: &Block, _height: u32) -> Vec<Txid> {
        let mut result = Vec::with_capacity(block.txdata.len());
        for tx in block.txdata.iter() {
            let txid = tx.txid();
            for (i, output) in tx.output.iter().enumerate() {
                let outpoint = OutPoint::new(txid, i as u32);
                self.db
                    .put(serialize(&outpoint), serialize(output))
                    .unwrap(); //TODO use batch, remove unwrap
            }
            result.push(txid);
        }
        //TODO call remove for every input and save result in height
        result
    }

    fn remove(&mut self, outpoint: &OutPoint) -> TxOut {
        deserialize(&self.db.get(serialize(&outpoint)).unwrap().unwrap()).unwrap()
    }

    fn get_all(&self, _height: u32) -> Option<Vec<TxOut>> {
        //TODO implement
        None
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

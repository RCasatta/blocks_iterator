use crate::bitcoin::consensus::serialize;
use crate::bitcoin::{Block, OutPoint, TxOut};
use crate::utxo::UtxoStore;
use bitcoin::consensus::{deserialize, Encodable};
use log::{debug, info};
use rocksdb::{DBCompressionType, Options, WriteBatch, DB};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;

pub struct DbUtxo {
    db: DB,
    updated_up_to_height: i32,
    inserted_outputs: u64,
    script_buffer: [u8; 10_000],
    outpoint_buffer: [u8; 36],
}
// use column family to separate things in the db

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

        info!("DB updated_height: {}", updated_up_to_height);

        Ok(DbUtxo {
            db,
            updated_up_to_height,
            inserted_outputs: 0,
            script_buffer: [0u8; 10_000],
            outpoint_buffer: [0u8; 36],
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
                            input
                                .previous_output
                                .consensus_encode(&mut self.outpoint_buffer[..])
                                .unwrap();
                            let tx_out = deserialize(
                                &self.db.get_pinned(&self.outpoint_buffer).unwrap().unwrap(),
                            )
                            .unwrap();
                            batch.delete(&self.outpoint_buffer);
                            prevouts.push(tx_out);
                        }
                    }
                }
            }

            // and we put all the remaining outputs in db
            for (k, v) in block_outputs.drain() {
                k.consensus_encode(&mut self.outpoint_buffer[..]).unwrap();
                let script_len = v.consensus_encode(&mut self.script_buffer[..]).unwrap();
                batch.put(&self.outpoint_buffer[..], &self.script_buffer[..script_len]);
                self.inserted_outputs += 1;
            }
            batch.put(height.to_ne_bytes(), serialize(&prevouts));
            batch.put(UPDATED_UP_TO_HEIGHT, height.to_ne_bytes());
            self.db.write(batch).unwrap(); // TODO unwrap
            prevouts
        } else {
            self.db
                .get(height.to_ne_bytes())
                .unwrap()
                .map(|e| deserialize(&e).unwrap())
                .unwrap()
        }
    }

    fn stat(&self) -> String {
        format!(
            "updated_up_to_height: {} inserted_outputs: {}",
            self.updated_up_to_height, self.inserted_outputs
        )
    }
}

#[cfg(all(test, feature = "unstable"))]
mod bench {

    use rocksdb::{Options, WriteBatch, DB};
    use test::Bencher;
    #[bench]
    fn bench_db_batch(b: &mut Bencher) {
        let tempdir = tempfile::TempDir::new().unwrap();
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, &tempdir).unwrap();

        b.iter(|| {
            let mut key = [0u8; 32];
            let value = [0u8; 32];
            let mut batch = WriteBatch::default();
            for i in 0..200 {
                key[i as usize % 32] = i;
                batch.put(key, value);
            }
            db.write(batch).unwrap();
            db.flush().unwrap();
        });
    }

    #[bench]
    fn bench_db_no_batch(b: &mut Bencher) {
        let tempdir = tempfile::TempDir::new().unwrap();
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, &tempdir).unwrap();
        b.iter(|| {
            let mut key = [0u8; 32];
            let value = [0u8; 32];
            for i in 0..200 {
                key[i as usize % 32] = i;
                db.put(key, value).unwrap();
            }
            db.flush().unwrap();
        });
    }
}

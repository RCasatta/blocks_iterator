use crate::bitcoin::consensus::serialize;
use crate::bitcoin::{Block, OutPoint, TxOut};
use crate::utxo::UtxoStore;
use bitcoin::consensus::{deserialize, Encodable};
use log::{debug, info};
use rocksdb::{Options, WriteBatch, DB};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;

pub struct DbUtxo {
    db: DB,
    updated_up_to_height: i32,
    inserted_outputs: u64,
}

/// This prefix contains currently unspent transaction outputs.
const UTXO_PREFIX: u8 = 'U' as u8;

/// This prefix contains all prevouts of a given block.
const PREVOUTS_PREFIX: u8 = 'P' as u8;

/// This prefix contains the height meanint the db updated up to this.
const HEIGHT_PREFIX: u8 = 'H' as u8;

impl DbUtxo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<DbUtxo, rocksdb::Error> {
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, path)?;

        let updated_up_to_height = db
            .get(&[HEIGHT_PREFIX])?
            .map(|e| e.try_into().unwrap())
            .map(|e| i32::from_ne_bytes(e))
            .unwrap_or(-1);

        info!("DB updated_height: {}", updated_up_to_height);

        Ok(DbUtxo {
            db,
            updated_up_to_height,
            inserted_outputs: 0,
        })
    }
}

fn serialize_outpoint(o: &OutPoint, buffer: &mut [u8; 37]) {
    buffer[0] = UTXO_PREFIX;
    o.consensus_encode(&mut buffer[1..]).unwrap();
}

fn serialize_txout(o: &TxOut, buffer: &mut [u8; 10_011]) -> usize {
    // No need to prefix, used
    o.consensus_encode(&mut buffer[..]).unwrap()
}

fn serialize_prevouts_height(h: i32) -> [u8; 5] {
    let mut ser = [PREVOUTS_PREFIX, 0, 0, 0, 0];
    h.consensus_encode(&mut ser[1..]).unwrap();
    ser
}

impl UtxoStore for DbUtxo {
    fn add_outputs_get_inputs(&mut self, block: &Block, height: u32) -> Vec<TxOut> {
        let mut outpoint_buffer = [0u8; 37]; // prefix(1) + txid (32) + vout (4)
        let mut txout_buffer = [0u8; 10_011]; // max(script) (10_000) +  max(varint) (3) + value (8)  (there are exceptions, see where used)

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
                            serialize_outpoint(&input.previous_output, &mut outpoint_buffer);
                            let tx_out = deserialize(
                                &self.db.get_pinned(&outpoint_buffer).unwrap().unwrap(),
                            )
                            .unwrap();
                            batch.delete(&outpoint_buffer);
                            prevouts.push(tx_out);
                        }
                    }
                }
            }

            // and we put all the remaining outputs in db
            for (k, v) in block_outputs.drain() {
                serialize_outpoint(&k, &mut outpoint_buffer);
                if v.script_pubkey.len() <= 10_000 {
                    // max script size for spendable output is 10k https://bitcoin.stackexchange.com/a/35881/6693 ...
                    let used = serialize_txout(v, &mut txout_buffer);
                    batch.put(&outpoint_buffer[..], &txout_buffer[..used]);
                } else {
                    // ... however there are bigger unspendable output like testnet 73e64e38faea386c88a578fd1919bcdba3d0b3af7b6302bf6ee1b423dc4e4333:0
                    // this rare case are handled separately here, this is less perfomant because `serialize` allocates a vector
                    info!(
                        "script len > 10_000: {} outpoint:{:?}",
                        v.script_pubkey.len(),
                        k
                    );
                    batch.put(&outpoint_buffer[..], &serialize(&v));
                }
                self.inserted_outputs += 1;
            }
            batch.put(serialize_prevouts_height(height), serialize(&prevouts)); // TODO consider compress this value
            batch.put(&[HEIGHT_PREFIX], height.to_ne_bytes());
            self.db.write(batch).unwrap(); // TODO unwrap
            prevouts
        } else {
            self.db
                .get(serialize_prevouts_height(height))
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

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_ser() {
        assert_eq!([PREVOUTS_PREFIX, 1, 0, 0, 0], serialize_prevouts_height(1));

        let mut outpoint_buffer = [0u8; 37];
        serialize_outpoint(&OutPoint::default(), &mut outpoint_buffer);
        let mut expected = [0u8; 37];
        expected[0] = UTXO_PREFIX;
        for i in 33..37 {
            expected[i] = 0xFF_u8;
        }
        assert_eq!(expected, outpoint_buffer);
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

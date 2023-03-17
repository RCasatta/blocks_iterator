use crate::bitcoin::consensus::serialize;
use crate::bitcoin::{OutPoint, TxOut};
use crate::utxo::UtxoStore;
use crate::BlockExtra;
use bitcoin::consensus::deserialize;
use bitcoin_slices::redb::{self, Database, ReadableTable, TableDefinition};
use bitcoin_slices::{bsl, Parse};
use log::{debug, info};
use std::collections::HashMap;
use std::path::Path;

pub struct DbUtxo {
    db: Database,
    updated_up_to_height: i32,
    inserted_outputs: u64,
}

/// This table contains currently (up to the height defined in INTS_TABLE) unspent transaction outputs.
const UTXOS_TABLE: TableDefinition<bsl::OutPoint, bsl::TxOut> = TableDefinition::new("utxos");

/// This table contains all prevouts of a given block.
const PREVOUTS_TABLE: TableDefinition<i32, bsl::TxOuts> = TableDefinition::new("prevouts");

/// This table contains the height meaning the db updated up to this.
const INTS_TABLE: TableDefinition<&str, i32> = TableDefinition::new("ints");

impl DbUtxo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<DbUtxo, redb::Error> {
        let db = Database::create(path)?;

        let tables: Vec<_> = {
            let read_txn = db.begin_read()?;
            read_txn.list_tables()?.collect()
        };
        if tables.len() != 3 {
            let write_txn = db.begin_write()?;
            write_txn.open_table(UTXOS_TABLE)?;
            write_txn.open_table(PREVOUTS_TABLE)?;
            write_txn.open_table(INTS_TABLE)?;
            write_txn.commit()?;
        }

        let updated_up_to_height = {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(INTS_TABLE)?;
            let result = table.get("height")?;
            result.map(|a| a.value()).unwrap_or(-1)
        };

        info!("DB updated_height: {}", updated_up_to_height);

        Ok(DbUtxo {
            db,
            updated_up_to_height,
            inserted_outputs: 0,
        })
    }
}

impl UtxoStore for DbUtxo {
    fn add_outputs_get_inputs(&mut self, block_extra: &BlockExtra, height: u32) -> Vec<TxOut> {
        let block = &block_extra.block;
        // let mut outpoint_buffer = [0u8; 36]; // txid (32) + vout (4)

        // max script size for spendable output is 10k https://bitcoin.stackexchange.com/a/35881/6693 ...
        // let mut txout_buffer = [0u8; 10_011]; // max(script) (10_000) +  max(varint) (3) + value (8)  (there are exceptions, see where used)

        let height = height as i32;
        debug!(
            "height: {} updated_up_to: {}",
            height, self.updated_up_to_height
        );
        if height > self.updated_up_to_height {
            // since we can spend outputs created in this same block, we first put outputs in memory...
            let total_outputs = block.txdata.iter().map(|e| e.output.len()).sum();
            let mut block_outputs = HashMap::with_capacity(total_outputs);
            for (txid, tx) in block_extra.iter_tx() {
                for (i, output) in tx.output.iter().enumerate() {
                    if !output.script_pubkey.is_provably_unspendable() {
                        let outpoint = OutPoint::new(*txid, i as u32);
                        block_outputs.insert(outpoint, output);
                    }
                }
            }

            let total_inputs = block.txdata.iter().skip(1).map(|e| e.input.len()).sum();
            let mut prevouts = Vec::with_capacity(total_inputs);
            let mut to_delete = Vec::with_capacity(total_outputs);

            {
                let read_txn = self.db.begin_read().unwrap();
                let utxos_table = read_txn.open_table(UTXOS_TABLE).unwrap();

                for tx in block.txdata.iter().skip(1) {
                    for input in tx.input.iter() {
                        //...then we first check if inputs spend output created in this block
                        match block_outputs.remove(&input.previous_output) {
                            Some(tx_out) => {
                                // we avoid touching the db entirely if it's spent in the same block
                                prevouts.push(tx_out.clone())
                            }
                            None => {
                                let outpoint_bytes = serialize(&input.previous_output);
                                let out_point = bsl::OutPoint::parse(&outpoint_bytes)
                                    .unwrap()
                                    .parsed_owned();

                                let tx_out_slice = utxos_table.get(&out_point).unwrap().unwrap();
                                let tx_out = deserialize(tx_out_slice.value().as_ref()).unwrap();

                                to_delete.push(outpoint_bytes);
                                prevouts.push(tx_out);
                            }
                        }
                    }
                }
            }

            let write_txn = self.db.begin_write().unwrap();
            {
                let mut utxos_table = write_txn.open_table(UTXOS_TABLE).unwrap();

                for el in to_delete {
                    let out_point = bsl::OutPoint::parse(&el).unwrap().parsed_owned();
                    utxos_table.remove(out_point).unwrap();
                }

                // and we put all the remaining outputs in db
                for (k, v) in block_outputs.drain() {
                    let tx_out_bytes = serialize(&v);
                    let tx_out = bsl::TxOut::parse(&tx_out_bytes).unwrap().parsed_owned();
                    let out_point_bytes = serialize(&k);
                    let out_point = bsl::OutPoint::parse(&out_point_bytes)
                        .unwrap()
                        .parsed_owned();

                    utxos_table.insert(out_point, tx_out).unwrap();

                    self.inserted_outputs += 1;
                }
                if !prevouts.is_empty() {
                    // TODO consider compress this value serialized prevouts
                    let mut prevouts_table = write_txn.open_table(PREVOUTS_TABLE).unwrap();
                    let tx_outs_bytes = serialize(&prevouts);
                    let tx_outs = bsl::TxOuts::parse(&tx_outs_bytes).unwrap().parsed_owned();

                    prevouts_table.insert(height, tx_outs).unwrap();
                }
                let mut prevouts_table = write_txn.open_table(INTS_TABLE).unwrap();

                prevouts_table.insert("height", height).unwrap();
            }
            write_txn.commit().unwrap();
            prevouts
        } else {
            if block.txdata.len() == 1 {
                // avoid hitting disk when we have only the coinbase (no prevouts!)
                Vec::new()
            } else {
                let read_txn = self.db.begin_read().unwrap();
                let prevouts_table = read_txn.open_table(PREVOUTS_TABLE).unwrap();
                let tx_outs = prevouts_table.get(height).unwrap().unwrap();
                deserialize(tx_outs.value().as_ref()).unwrap()
            }
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

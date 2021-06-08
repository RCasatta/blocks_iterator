use crate::truncmap::TruncMap;
use crate::BlockExtra;
use bitcoin::{OutPoint, Script, TxOut};
use log::{debug, info, trace};
use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Fee {
    receiver: Receiver<Option<BlockExtra>>,
    sender: SyncSender<Option<BlockExtra>>,
    utxo: Utxo,
}

struct Utxo {
    map: TruncMap,
    unspendable: u64,
}

impl Utxo {
    pub fn new() -> Self {
        Utxo {
            map: TruncMap::default(),
            unspendable: 0,
        }
    }

    pub fn add(&mut self, out_point: OutPoint, output: &TxOut) {
        if output.script_pubkey.is_provably_unspendable() {
            self.unspendable += 1;
        } else {
            self.map.insert(out_point, output);
        }
    }

    pub fn remove(&mut self, outpoint: OutPoint) -> TxOut {
        self.map.remove(&outpoint).unwrap()
    }
}

impl Fee {
    pub fn new(
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Fee {
        Fee {
            sender,
            receiver,
            utxo: Utxo::new(),
        }
    }

    pub fn start(&mut self) {
        info!("starting fee processer");
        let mut busy_time = 0u128;
        let mut total_txs = 0u64;
        loop {
            let received = self.receiver.recv().unwrap();
            let now = Instant::now();
            match received {
                Some(mut block_extra) => {
                    trace!("fee received: {}", block_extra.block_hash);
                    total_txs += block_extra.block.txdata.len() as u64;

                    if block_extra.height % 10_000 == 0 {
                        let (utxo_size, collision_size, utxo_capacity) = self.utxo.map.len();
                        info!(
                            "(utxo, collision, capacity): {:?} load:{:.1}% script on stack: {:.1}% unspendable:{}",
                            (utxo_size, collision_size, utxo_capacity),
                            (utxo_size as f64 / utxo_capacity as f64) * 100.0,
                            self.utxo.map.script_on_stack() * 100.0,
                            self.utxo.unspendable,
                        );
                    }

                    // spent_in_block is an optimization to avoid hitting the utxo map with outputs
                    // spent in the same block, should improve caching by using a smaller data struct
                    // and avoid the double lookup of the trunc map
                    let spent_in_block: HashSet<_> = block_extra
                        .block
                        .txdata
                        .iter()
                        .flat_map(|a| a.input.iter())
                        .map(|a| &a.previous_output)
                        .collect();
                    for tx in block_extra.block.txdata.iter() {
                        let txid = tx.txid();
                        for (i, output) in tx.output.iter().enumerate() {
                            let out_point = OutPoint::new(txid, i as u32);
                            if spent_in_block.contains(&out_point) {
                                block_extra
                                    .outpoint_values
                                    .insert(out_point, output.clone());
                            } else {
                                self.utxo.add(out_point, output);
                            }
                        }

                        block_extra.tx_hashes.insert(txid);
                    }

                    for tx in block_extra.block.txdata.iter().skip(1) {
                        for input in tx.input.iter() {
                            let previous_txout = self.utxo.remove(input.previous_output);
                            block_extra
                                .outpoint_values
                                .insert(input.previous_output, previous_txout);
                        }
                    }
                    let coin_base_output_value = block_extra.block.txdata[0]
                        .output
                        .iter()
                        .map(|el| el.value)
                        .sum();
                    block_extra.outpoint_values.insert(
                        OutPoint::default(),
                        TxOut {
                            script_pubkey: Script::new(),
                            value: coin_base_output_value,
                        },
                    );

                    debug!(
                        "#{:>6} {} size:{:>7} txs:{:>4} total_txs:{:>9} fee:{:?}",
                        block_extra.height,
                        block_extra.block_hash,
                        block_extra.size,
                        block_extra.block.txdata.len(),
                        total_txs,
                        block_extra.fee(),
                    );

                    busy_time += now.elapsed().as_nanos();
                    self.sender.send(Some(block_extra)).unwrap();
                }
                None => break,
            }
        }

        self.sender.send(None).expect("fee: cannot send none");

        info!(
            "ending fee processer total tx {}, busy time: {}s",
            total_txs,
            busy_time / 1_000_000_000
        );
    }
}

#[cfg(test)]
mod test {
    use bitcoin::TxOut;

    #[test]
    fn test_size() {
        assert_eq!(
            std::mem::size_of::<Option<TxOut>>(),
            std::mem::size_of::<TxOut>()
        );
    }
}

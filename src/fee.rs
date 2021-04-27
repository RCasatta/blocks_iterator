use crate::truncmap::TruncMap;
use crate::BlockExtra;
use bitcoin::{OutPoint, Script, Transaction, TxOut, Txid};
use log::{debug, info, trace};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Fee {
    receiver: Receiver<Option<BlockExtra>>,
    sender: SyncSender<Option<BlockExtra>>,
    utxo: Utxo,
}

struct Utxo(TruncMap);

impl Utxo {
    pub fn new() -> Self {
        Utxo(TruncMap::default())
    }

    pub fn add(&mut self, tx: &Transaction) -> Txid {
        let txid = tx.txid();
        for (i, output) in tx.output.iter().enumerate() {
            self.0.insert(OutPoint::new(txid, i as u32), output);
        }
        txid
    }

    pub fn remove(&mut self, outpoint: OutPoint) -> TxOut {
        self.0.remove(&outpoint).unwrap()
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
                        info!("(outpoints, outpoints_collision, scripts): {:?}", self.utxo.0.len())
                    }
                    for tx in block_extra.block.txdata.iter() {
                        let txid = self.utxo.add(tx);
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

use crate::BlockExtra;
use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, Script, Transaction, TxOut, Txid};
use log::{debug, info};
use fxhash::FxHashMap;
use std::convert::TryInto;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Fee {
    receiver: Receiver<Option<BlockExtra>>,
    sender: SyncSender<Option<BlockExtra>>,
    utxo: Utxo,
}

struct Utxo(FxHashMap<TruncatedHash, Vec<Option<TxOut>>>);

#[derive(Eq, PartialEq, Hash)]
struct TruncatedHash([u8; 12]);

impl From<Txid> for TruncatedHash {
    fn from(txid: Txid) -> Self {
        TruncatedHash(txid.into_inner()[0..12].try_into().unwrap())
    }
}

impl Utxo {
    pub fn new() -> Self {
        Utxo(FxHashMap::default())
    }

    pub fn add(&mut self, tx: &Transaction) -> Txid {
        let txid = tx.txid();
        self.0.insert(
            txid.into(),
            tx.output.iter().map(|txout| Some(txout.clone())).collect(),
        );
        txid
    }

    pub fn get(&mut self, outpoint: OutPoint) -> TxOut {
        let truncated: TruncatedHash = outpoint.txid.into();
        let mut outputs = self.0.remove(&truncated).unwrap();
        let value = outputs[outpoint.vout as usize].take().unwrap();
        if outputs.iter().any(|e| e.is_some()) {
            self.0.insert(truncated, outputs);
        }
        value
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
                    debug!("fee received: {}", block_extra.block_hash);
                    total_txs += block_extra.block.txdata.len() as u64;
                    for tx in block_extra.block.txdata.iter() {
                        let txid = self.utxo.add(tx);
                        block_extra.tx_hashes.insert(txid);
                    }

                    for tx in block_extra.block.txdata.iter().skip(1) {
                        for input in tx.input.iter() {
                            let previous_txout = self.utxo.get(input.previous_output);
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
                        "#{:>6} {} size:{:>7} txs:{:>4} total_txs:{:>9} fee:{:>9}",
                        block_extra.height,
                        block_extra.block_hash,
                        block_extra.size,
                        block_extra.block.txdata.len(),
                        total_txs,
                        block_extra.fee(),
                    );
                    busy_time = busy_time + now.elapsed().as_nanos();
                    self.sender
                        .send(Some(block_extra))
                        .expect("fee: cannot send");
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

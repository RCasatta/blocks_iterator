use crate::truncmap::TruncMap;
use crate::BlockExtra;
use bitcoin::hashes::hex::FromHex;
use bitcoin::{OutPoint, Script, Transaction, TxOut, Txid};
use log::{debug, info, trace};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Fee {
    skip_prevout: bool,
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
        let prev = self.0.insert(
            txid,
            tx.output.iter().map(|txout| Some(txout.clone())).collect(),
        );
        if prev.is_some() {
            // pre bip-34 issue, coinbase without height may create the same hash
            let a = "d5d27987d2a3dfc724e359870c6644b40e497bdc0589a033220fe15429d88599";
            let b = "e3bf3d07d4b0375638d5f1db5255fe07ba2c4cb067cd81b84ee974b6585fb468";
            if txid != Txid::from_hex(a).unwrap() && txid != Txid::from_hex(b).unwrap() {
                panic!("truncated hash caused a collision {}", txid);
            }
        }

        txid
    }

    pub fn get(&mut self, outpoint: OutPoint) -> TxOut {
        let mut outputs = self.0.remove(&outpoint.txid).unwrap();
        let value = outputs[outpoint.vout as usize].take().unwrap();
        if outputs.iter().any(|e| e.is_some()) {
            self.0.insert(outpoint.txid, outputs);
        }
        value
    }
}

impl Fee {
    pub fn new(
        skip_prevout: bool,
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Fee {
        Fee {
            skip_prevout,
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
                    if !self.skip_prevout {
                        if block_extra.height % 10_000 == 0 {
                            info!("tx in utxo: {:?}", self.utxo.0.len())
                        }
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
                    }
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

use crate::utxo::Utxo;
use crate::BlockExtra;
use bitcoin::{OutPoint, Script, TxOut};
use log::{debug, info, trace};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Fee<T: Utxo> {
    receiver: Receiver<Option<BlockExtra>>,
    sender: SyncSender<Option<BlockExtra>>,
    utxo: T,
}

impl<T: Utxo> Fee<T> {
    pub fn new(
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
        utxo: T,
    ) -> Fee<T> {
        Fee {
            sender,
            receiver,
            utxo,
        }
    }

    pub fn start(&mut self) {
        info!("starting fee processer");
        let mut now = Instant::now();
        let mut busy_time = 0u128;
        let mut total_txs = 0u64;
        let mut last_height = 0;
        loop {
            busy_time += now.elapsed().as_nanos();
            let received = self.receiver.recv().unwrap();
            now = Instant::now();
            match received {
                Some(mut block_extra) => {
                    last_height = block_extra.height;
                    trace!("fee received: {}", block_extra.block_hash);
                    total_txs += block_extra.block.txdata.len() as u64;

                    if block_extra.height % 10_000 == 0 {
                        info!("{}", self.utxo.stat());
                    }

                    self.utxo.add(&block_extra.block, block_extra.height);

                    for tx in block_extra.block.txdata.iter().skip(1) {
                        for input in tx.input.iter() {
                            let previous_txout = self.utxo.remove(&input.previous_output);
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
                    now = Instant::now();
                }
                None => break,
            }
        }
        info!(
            "ending fee processer total tx {}, busy time: {}s, last height: {}",
            total_txs,
            busy_time / 1_000_000_000,
            last_height
        );
        self.sender.send(None).expect("fee: cannot send none");
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

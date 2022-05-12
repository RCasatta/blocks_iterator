use crate::utxo::UtxoStore;
use crate::{BlockExtra, Periodic};
use bitcoin::{OutPoint, Script, TxOut};
use log::{debug, info, trace};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct Fee {
    join: Option<JoinHandle<()>>,
}

impl Drop for Fee {
    fn drop(&mut self) {
        if let Some(jh) = self.join.take() {
            jh.join().expect("thread failed");
        }
    }
}

impl Fee {
    pub fn new<T: 'static + UtxoStore + Send>(
        start_at_height: u32,
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
        mut utxo: T,
    ) -> Self {
        Self {
            join: Some(std::thread::spawn(move || {
                info!("starting fee processer");
                let mut now = Instant::now();
                let mut busy_time = 0u128;
                let mut total_txs = 0u64;
                let mut last_height = 0;
                let mut periodic = Periodic::new(Duration::from_secs(60));
                loop {
                    busy_time += now.elapsed().as_nanos();
                    let received = receiver.recv().unwrap();
                    now = Instant::now();
                    match received {
                        Some(mut block_extra) => {
                            last_height = block_extra.height;
                            trace!("fee received: {}", block_extra.block_hash);
                            total_txs += block_extra.block.txdata.len() as u64;

                            let mut prevouts =
                                utxo.add_outputs_get_inputs(&block_extra.block, block_extra.height);
                            let mut prevouts = prevouts.drain(..);

                            for tx in block_extra.block.txdata.iter().skip(1) {
                                for input in tx.input.iter() {
                                    let previous_txout = prevouts.next().unwrap();

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

                            if periodic.elapsed() {
                                info!("{}", utxo.stat());
                                info!(
                                    "# {:7} {} fee: {:?}",
                                    block_extra.height,
                                    block_extra.block_hash,
                                    block_extra.fee()
                                );
                            }

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
                            if block_extra.height >= start_at_height {
                                sender.send(Some(block_extra)).unwrap();
                            }
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
                sender.send(None).expect("fee: cannot send none");
            })),
        }
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

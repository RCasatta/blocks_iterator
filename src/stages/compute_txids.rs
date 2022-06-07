use crate::BlockExtra;
use log::info;
use rayon::prelude::*;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

pub struct ComputeTxids {
    join: Option<JoinHandle<()>>,
}

impl Drop for ComputeTxids {
    fn drop(&mut self) {
        if let Some(jh) = self.join.take() {
            jh.join().expect("thread failed");
        }
    }
}

impl ComputeTxids {
    pub fn new(
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Self {
        Self {
            join: Some(std::thread::spawn(move || {
                info!("starting augment processer");
                let mut now = Instant::now();
                let mut busy_time = Duration::default();
                loop {
                    busy_time += now.elapsed();
                    let received = receiver.recv().unwrap();
                    now = Instant::now();
                    match received {
                        Some(mut block_extra) => {
                            block_extra.compute_txids();
                            busy_time += now.elapsed();
                            sender.send(Some(block_extra)).unwrap();
                            now = Instant::now();
                        }
                        None => break,
                    }
                }
                info!("ending augment processer busy time: {:?}", busy_time,);
                sender.send(None).expect("augment: cannot send none");
            })),
        }
    }
}

impl BlockExtra {
    fn compute_txids(&mut self) {
        if !self.txids.is_empty() {
            return;
        }

        self.txids = self.block.txdata.par_iter().map(|tx| tx.txid()).collect();
    }
}

use crate::BlockExtra;
use log::info;
use rayon::prelude::*;
use rayon::ThreadPool;
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
                info!("starting compute tx ids");
                let mut now = Instant::now();
                let mut busy_time = Duration::default();
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(4)
                    .build()
                    .unwrap();
                loop {
                    busy_time += now.elapsed();
                    let received = receiver.recv().unwrap();
                    now = Instant::now();
                    match received {
                        Some(mut block_extra) => {
                            block_extra.compute_txids(&pool);
                            busy_time += now.elapsed();
                            sender.send(Some(block_extra)).unwrap();
                            now = Instant::now();
                        }
                        None => break,
                    }
                }
                info!("ending compute tx ids busy time: {:?}", busy_time,);
                sender.send(None).expect("augment: cannot send none");
            })),
        }
    }
}

impl BlockExtra {
    fn compute_txids(&mut self, pool: &ThreadPool) {
        if !self.txids.is_empty() {
            return;
        }
        // without using a thread pool it may interact badly with library consumer using rayon,
        // causing deadlock on the global thread pool
        self.txids = pool.install(|| self.block.txdata.par_iter().map(|tx| tx.txid()).collect());
    }
}

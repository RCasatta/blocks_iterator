use crate::BlockExtra;
use bitcoin::Txid;
use bitcoin_slices::bsl;
use bitcoin_slices::Visit;
use bitcoin_slices::Visitor;
use log::info;
use std::ops::ControlFlow;
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
        skip_prevout: bool,
        start_at_height: u32,
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Self {
        Self {
            join: Some(std::thread::spawn(move || {
                info!("starting compute tx ids");
                let mut now = Instant::now();
                let mut busy_time = Duration::default();
                loop {
                    busy_time += now.elapsed();
                    let received = receiver.recv().unwrap();
                    now = Instant::now();
                    match received {
                        Some(mut block_extra) => {
                            if !skip_prevout || block_extra.height >= start_at_height {
                                // always send if we are not skipping prevouts, otherwise only if height is enough
                                block_extra.compute_txids();
                                busy_time += now.elapsed();
                                sender.send(Some(block_extra)).unwrap();
                                now = Instant::now();
                            }
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
    fn compute_txids(&mut self) {
        if !self.txids.is_empty() {
            return;
        }

        let mut visitor = TxidsVisitor::new(); // TODO add tx_count to block_extra and use it as capacity
        bsl::Block::visit(self.block_bytes(), &mut visitor).expect("compute txids");
        self.txids = visitor.txids;
        self.block_total_txs = self.txids.len();
    }
}

struct TxidsVisitor {
    txids: Vec<Txid>,
}

impl TxidsVisitor {
    fn new() -> Self {
        Self { txids: vec![] }
    }
}

impl Visitor for TxidsVisitor {
    fn visit_transaction(&mut self, tx: &bsl::Transaction) -> ControlFlow<()> {
        self.txids.push(tx.txid().into());
        ControlFlow::Continue(())
    }
}

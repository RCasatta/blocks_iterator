use bitcoin::consensus::{deserialize, serialize};
use bitcoin::{Amount, Script, Transaction};
use bitcoinconsensus::height_to_flags;
use blocks_iterator::{Config, PeriodCounter};
use env_logger::Env;
use log::{error, info};
use rayon::prelude::*;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use structopt::StructOpt;

#[derive(Debug)]
struct VerifyData {
    script_pubkey: Script,
    index: usize,
    amount: Amount,
    spending: Arc<Vec<u8>>,
    flags: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();
    let (send, recv) = sync_channel(config.channels_size.into());
    let (send_script, recv_script) = sync_channel(config.channels_size.into());
    let mut period = PeriodCounter::new(Duration::from_secs(10));

    let handle = blocks_iterator::iterate(config, send);

    let process_handle = thread::spawn(move || {
        let error_count = AtomicUsize::new(0);
        recv_script
            .into_iter()
            .par_bridge()
            .for_each(|data: VerifyData| {
                if let Err(e) = data.script_pubkey.verify_with_flags(
                    data.index,
                    data.amount,
                    &data.spending,
                    data.flags,
                ) {
                    error!("{:?}", e);
                    error!("{:?}", data);
                    let tx: Transaction = deserialize(&data.spending).unwrap();
                    error!("tx: {}", tx.txid());
                    error_count.fetch_add(1, Ordering::SeqCst);
                }
            })
    });

    while let Some(mut block_extra) = recv.recv()? {
        if period.period_elapsed().is_some() {
            info!(
                "# {:7} {} {:?}",
                block_extra.height,
                block_extra.block_hash,
                block_extra.fee()
            );
        }
        for tx in block_extra.block.txdata.iter().skip(1) {
            let tx_bytes = serialize(tx);
            let arc_tx_bytes = Arc::new(tx_bytes);
            for (i, input) in tx.input.iter().enumerate() {
                let prevout = block_extra
                    .outpoint_values
                    .remove(&input.previous_output)
                    .unwrap();
                let data = VerifyData {
                    script_pubkey: prevout.script_pubkey,
                    index: i,
                    amount: Amount::from_sat(prevout.value),
                    spending: arc_tx_bytes.clone(),
                    flags: height_to_flags(block_extra.height),
                };
                send_script.send(data)?;
            }
        }
    }
    drop(send_script);
    process_handle.join().expect("couldn't join");
    handle.join().expect("couldn't join");

    Ok(())
}

use bitcoin::consensus::{deserialize, serialize};
use bitcoin::{Amount, Script, Transaction};
use bitcoinconsensus::height_to_flags;
use blocks_iterator::{periodic_log_level, Config};
use env_logger::Env;
use log::{error, info, log};
use rayon::prelude::*;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::thread;
use structopt::StructOpt;

const BATCH: usize = 10_000;

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
    let (send, recv) = sync_channel(0);
    let handle = blocks_iterator::iterate(config, send);

    let (send_script, recv_script) = sync_channel(BATCH);

    let process_handle = thread::spawn(move || {
        let error_count = AtomicUsize::new(0);
        let mut buffer: Vec<VerifyData> = Vec::with_capacity(BATCH);
        let mut last = false;
        loop {
            for _ in 0..BATCH {
                match recv_script.recv().unwrap() {
                    Some(data) => buffer.push(data),
                    None => {
                        last = true;
                        break;
                    }
                }
            }

            buffer.par_iter().for_each(|data| {
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
            });
            if last {
                break;
            }
            buffer.clear();
        }
        println!("errors: {:?}", error_count);
    });

    while let Some(mut block_extra) = recv.recv()? {
        log!(
            periodic_log_level(block_extra.height),
            "# {:7} {} {:?}",
            block_extra.height,
            block_extra.block_hash,
            block_extra.fee()
        );
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
                send_script.send(Some(data))?;
            }
        }
    }
    send_script.send(None)?;
    process_handle.join().expect("couldn't join");
    handle.join().expect("couldn't join");

    Ok(())
}

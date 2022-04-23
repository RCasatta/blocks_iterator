use bitcoin::consensus::{deserialize, serialize};
use bitcoin::{Amount, Script, Transaction};
use bitcoinconsensus::height_to_flags;
use blocks_iterator::{BlockExtra, Config};
use env_logger::Env;
use log::{debug, error, info};
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use structopt::StructOpt;

#[derive(Debug)]
struct VerifyData {
    script_pubkey: Script,
    index: usize,
    amount: Amount,
    spending: Arc<Vec<u8>>,
    flags: u32,
}

fn pre_processing(mut block_extra: BlockExtra) -> Vec<VerifyData> {
    let mut vec = vec![];
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
            vec.push(data);
        }
    }
    debug!("end preprocessing, #elements: {}", vec.len());
    vec
}

fn task(data: VerifyData, error_count: Arc<AtomicUsize>) -> bool {
    debug!("task");

    if let Err(e) =
        data.script_pubkey
            .verify_with_flags(data.index, data.amount, &data.spending, data.flags)
    {
        error!("{:?}", e);
        error!("{:?}", data);
        let tx: Transaction = deserialize(&data.spending).unwrap();
        error!("tx: {}", tx.txid());
        error_count.fetch_add(1, Ordering::SeqCst);
    }
    false
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();

    let state = Arc::new(AtomicUsize::new(0));

    blocks_iterator::par_iter(config, state.clone(), pre_processing, task);

    info!("error count: {}", state.load(Ordering::SeqCst));

    Ok(())
}

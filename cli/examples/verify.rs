use bitcoin::consensus::{deserialize, serialize};
use bitcoin::{Amount, ScriptBuf, Transaction};
use bitcoinconsensus::height_to_flags;
use blocks_iterator::{BlockExtra, Config};
use clap::Parser;
use env_logger::Env;
use log::{debug, error, info};
use rayon::iter::{ParallelBridge, ParallelIterator};
use std::error::Error;
use std::sync::Arc;

#[derive(Debug)]
struct VerifyData {
    script_pubkey: ScriptBuf,
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
                amount: prevout.value,
                spending: arc_tx_bytes.clone(),
                flags: height_to_flags(block_extra.height),
            };
            vec.push(data);
        }
    }
    debug!("end preprocessing, #elements: {}", vec.len());
    vec
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::parse();

    let errors: Vec<_> = blocks_iterator::iter(config)
        .flat_map(pre_processing)
        .par_bridge()
        .filter_map(|d| {
            match d
                .script_pubkey
                .verify_with_flags(d.index, d.amount, &d.spending, d.flags)
            {
                Err(e) => Some((d, e)),
                _ => None,
            }
        })
        .collect();

    for (data, e) in errors {
        error!("{:?}", e);
        error!("{:?}", data);
        let tx: Transaction = deserialize(&data.spending).unwrap();
        error!("tx: {}", tx.compute_txid());
    }

    Ok(())
}

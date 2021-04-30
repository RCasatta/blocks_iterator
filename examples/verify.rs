use bitcoin::consensus::serialize;
use bitcoin::hashes::hex::ToHex;
use bitcoin::OutPoint;
use blocks_iterator::{periodic_log_level, Config};
use env_logger::Env;
use log::{error, info, log};
use std::error::Error;
use std::sync::mpsc::sync_channel;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();
    let (send, recv) = sync_channel(100);
    let handle = blocks_iterator::iterate(config, send);
    let mut err_count = 0;
    while let Some(block_extra) = recv.recv()? {
        log!(
            periodic_log_level(block_extra.height),
            "# {:7} {} {:?}",
            block_extra.height,
            block_extra.block_hash,
            block_extra.fee()
        );
        for tx in block_extra.block.txdata.iter().skip(1) {
            if let Err(e) = tx.verify_with_flags(
                |point: &OutPoint| block_extra.outpoint_values.get(point).cloned(),
                0u32,
            ) {
                //TODO initializa flags correctly (now 0 is none)
                error!(
                    "err_{} {:?} {} {}",
                    err_count,
                    e,
                    tx.txid(),
                    serialize(tx).to_hex()
                );
                for (i, input) in tx.input.iter().enumerate() {
                    let prevout = block_extra
                        .outpoint_values
                        .get(&input.previous_output)
                        .unwrap();
                    error!(
                        "err_{} input_{}: value:{} script_pubkey:{:x} script_sig:{:x}",
                        err_count, i, prevout.value, prevout.script_pubkey, input.script_sig
                    );
                }
                err_count += 1;
                panic!("");
            }
        }
    }
    handle.join().expect("couldn't join");
    info!("error: {}", err_count);
    Ok(())
}
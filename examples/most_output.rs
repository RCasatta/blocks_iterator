use bitcoin::Txid;
use blocks_iterator::Config;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::sync::mpsc::sync_channel;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();
    let (send, recv) = sync_channel(0);
    let handle = blocks_iterator::iterate(config, send);
    let mut most_output: (Txid, usize) = (Txid::default(), 0);
    while let Some(block_extra) = recv.recv()? {
        for tx in block_extra.block.txdata.iter() {
            if tx.output.len() > most_output.1 {
                let txid = tx.txid();
                info!("New most_output tx: {}", txid);
                most_output = (txid, tx.output.len());
            }
        }
    }
    handle.join().expect("couldn't join");
    info!(
        "most_output tx is {} with #outputs: {}",
        most_output.0, most_output.1
    );
    Ok(())
}

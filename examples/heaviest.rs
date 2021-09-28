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
    let (send, recv) = sync_channel(config.channels_size.into());
    let handle = blocks_iterator::iterate(config, send);
    let mut heaviest: (Txid, usize) = (Txid::default(), 0);
    while let Some(block_extra) = recv.recv()? {
        for tx in block_extra.block.txdata.iter() {
            if tx.get_weight() > heaviest.1 {
                let txid = tx.txid();
                info!("New heaviest tx: {}", txid);
                heaviest = (txid, tx.get_weight());
            }
        }
    }
    handle.join().expect("couldn't join");
    info!("heaviest tx is {} with weight: {}", heaviest.0, heaviest.1);
    Ok(())
}

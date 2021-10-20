use bitcoin::Txid;
use blocks_iterator::PipeIterator;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::io;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let mut heaviest: (Txid, usize) = (Txid::default(), 0);

    let iter = PipeIterator::new(io::stdin(), io::stdout());

    for block_extra in iter {
        for tx in block_extra.block.txdata.iter() {
            if tx.get_weight() > heaviest.1 {
                let txid = tx.txid();
                info!("New heaviest tx: {}", txid);
                heaviest = (txid, tx.get_weight());
            }
        }
    }

    info!("heaviest tx is {} with weight: {}", heaviest.0, heaviest.1);
    Ok(())
}

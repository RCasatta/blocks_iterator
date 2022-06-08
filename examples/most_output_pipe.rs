use bitcoin::Txid;
use blocks_iterator::PipeIterator;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::io;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let mut most_output: (Txid, usize) = (Txid::default(), 0);

    let iter = PipeIterator::new(io::stdin(), Some(io::stdout()));

    for block_extra in iter {
        for (txid, tx) in block_extra.iter_tx() {
            if tx.output.len() > most_output.1 {
                info!("New most_output tx: {}", txid);
                most_output = (*txid, tx.output.len());
            }
        }
    }

    info!(
        "most_output tx is {} with #outputs: {}",
        most_output.0, most_output.1
    );
    Ok(())
}

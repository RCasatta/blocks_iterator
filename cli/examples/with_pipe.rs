use bitcoin::Weight;
use bitcoin::{hashes::Hash, BlockHash, Txid};
use blocks_iterator::PipeIterator;
use env_logger::Env;
use log::{info, warn};
use std::error::Error;
use std::io;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let iter = PipeIterator::new(io::stdin(), None);
    let mut total_missing_reward = 0u64;
    let mut blocks_missing_reward = 0u64;

    let mut block_most_tx: (BlockHash, usize) = (BlockHash::all_zeros(), 0);
    let mut most_output: (Txid, usize) = (Txid::all_zeros(), 0);
    let mut heaviest: (Txid, Weight) = (Txid::all_zeros(), Weight::ZERO);

    for block_extra in iter {
        let txs_fee = block_extra.fee().expect("launch without `--skip-prevout`");
        let block = &block_extra.block();
        let coinbase = &block.txdata[0];
        let coinbase_sum_outputs: u64 = coinbase
            .output
            .iter()
            .map(|output| output.value.to_sat())
            .sum();
        let base_reward = block_extra.base_reward();
        let missing_reward = base_reward + txs_fee - coinbase_sum_outputs;

        if missing_reward != 0 {
            blocks_missing_reward += 1;
            total_missing_reward += missing_reward;
            warn!(
                "block {} at height {} tx_fees:{}, coinbase_outputs:{}, missing_reward:{}",
                block.block_hash(),
                block_extra.height(),
                txs_fee,
                coinbase_sum_outputs,
                missing_reward
            );
        }

        let len = block.txdata.len();
        if len > block_most_tx.1 {
            info!(
                "New max number of txs: {} block: {:?}",
                len,
                block_extra.block_hash()
            );
            block_most_tx = (block_extra.block_hash(), len);
        }

        for (txid, tx) in block_extra.iter_tx() {
            if tx.output.len() > most_output.1 {
                info!("New most_output tx: {}", txid);
                most_output = (*txid, tx.output.len());
            }
            if tx.weight() > heaviest.1 {
                info!("New heaviest tx: {}", txid);
                heaviest = (*txid, tx.weight());
            }
        }
    }

    info!(
        "Max number of txs: {} block: {:?}",
        block_most_tx.1, block_most_tx.0
    );

    info!(
        "total missing reward: {} in {} blocks",
        total_missing_reward, blocks_missing_reward
    );

    info!(
        "most_output tx is {} with #outputs: {}",
        most_output.0, most_output.1
    );

    Ok(())
}

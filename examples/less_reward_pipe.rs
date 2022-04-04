use blocks_iterator::PipeIterator;
use env_logger::Env;
use log::{info, warn};
use std::error::Error;
use std::io;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let iter = PipeIterator::new(io::stdin(), Some(io::stdout()));
    let mut total_missing_reward = 0u64;
    let mut blocks_missing_reward = 0u64;

    for block_extra in iter {
        let txs_fee = block_extra.fee().expect("launch without `--skip-prevout`");
        let block = &block_extra.block;
        let coinbase = &block.txdata[0];
        let coinbase_sum_outputs: u64 = coinbase.output.iter().map(|output| output.value).sum();
        let base_reward = block_extra.base_reward();
        let missing_reward = base_reward + txs_fee - coinbase_sum_outputs;

        if missing_reward != 0 {
            blocks_missing_reward += 1;
            total_missing_reward += missing_reward;
            warn!(
                "block {} at height {} tx_fees:{}, coinbase_outputs:{}, missing_reward:{}",
                block.block_hash(),
                block_extra.height,
                txs_fee,
                coinbase_sum_outputs,
                missing_reward
            );
        }
    }
    info!(
        "total missing reward: {} in {} blocks",
        total_missing_reward, blocks_missing_reward
    );

    Ok(())
}

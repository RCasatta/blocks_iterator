use std::{
    sync::mpsc::{sync_channel, Receiver},
    thread::JoinHandle,
};

use log::error;

use crate::{iterate, BlockExtra, Config};

struct BlockExtraIterator {
    handle: Option<JoinHandle<()>>,
    recv: Receiver<Option<BlockExtra>>,
}
impl Iterator for BlockExtraIterator {
    type Item = BlockExtra;

    fn next(&mut self) -> Option<Self::Item> {
        match self.recv.recv() {
            Ok(Some(val)) => Some(val),
            Ok(None) => {
                if let Some(handle) = self.handle.take() {
                    handle.join().unwrap();
                }
                None
            }
            Err(e) => {
                error!("error iterating {:?}", e);
                if let Some(handle) = self.handle.take() {
                    handle.join().unwrap();
                }
                None
            }
        }
    }
}

/// Return an Iterator of [`BlockExtra`] read from `blocks*.dat` contained in the `config.blocks_dir`
/// Blocks returned are iterated in order, starting from the genesis to the highest block
/// (minus `config.max_reorg`) in the directory, unless `config.stop_at_height` is specified.
pub fn iter(config: Config) -> impl Iterator<Item = BlockExtra> {
    let (send, recv) = sync_channel(config.channels_size.into());

    let handle = Some(iterate(config, send));

    BlockExtraIterator { handle, recv }
}

#[deprecated(
    note = "you can get better and composable results by concateneting method on iter like 
    blocks_iterator::iter(config).flat_map(PREPROC).par_bridge().for_each(TASK)"
)]
/// `par_iter` is used when the task to be performed on the blockchain is more costly
/// than iterating the blocks. For example verifying the spending conditions in the blockchain.
/// like [`crate::iter`] accepts configuration parameters via the [`Config`] struct.
/// A `PREPROC` closure has to be provided, this process a single block and produce a Vec of
/// user defined struct `DATA`.
/// A `TASK` closure accepts the `DATA` struct and a shared `STATE` and it is executed in a
/// concurrent way. The `TASK` closure returns a bool which indicates if the execution should be
/// terminated.
/// Note that access to `STATE` in `TASK` should be done carefully otherwise the contention would
/// reduce the speed of execution.
///  
pub fn par_iter<STATE, PREPROC, TASK, DATA>(
    config: Config,
    state: std::sync::Arc<STATE>,
    pre_processing: PREPROC,
    task: TASK,
) where
    PREPROC: Fn(BlockExtra) -> Vec<DATA> + 'static + Send,
    TASK: Fn(DATA, std::sync::Arc<STATE>) -> bool + 'static + Send + Sync,
    DATA: 'static + Send,
    STATE: 'static + Send + Sync,
{
    use log::debug;
    use rayon::prelude::*;
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        thread,
    };

    let (send_task, recv_task) = sync_channel::<DATA>(config.channels_size.into());
    let stop = std::sync::Arc::new(AtomicBool::new(false));

    let stop_clone = stop.clone();
    let iter = iter(config);

    let handle = thread::spawn(move || {
        debug!("start pre-processing thread");
        for block_extra in iter {
            if stop_clone.load(Ordering::SeqCst) {
                break;
            }
            let data_vec = pre_processing(block_extra);
            for data in data_vec {
                send_task.send(data).unwrap();
            }
        }
        debug!("ending pre-processing thread");
    });

    recv_task.into_iter().par_bridge().for_each(|data: DATA| {
        debug!("iter");
        let result = task(data, state.clone());

        if result {
            stop.store(true, Ordering::SeqCst)
        }
    });
    handle.join().unwrap();
}

#[cfg(test)]
mod inner_test {
    use bitcoin::blockdata::constants::genesis_block;

    use super::*;
    use crate::bitcoin::Network;
    use crate::Config;

    fn test_conf() -> Config {
        Config {
            blocks_dir: "blocks".into(),
            network: Network::Testnet,
            skip_prevout: false,
            max_reorg: 10,
            channels_size: 0,
            #[cfg(feature = "db")]
            utxo_db: None,
            start_at_height: 0,
            stop_at_height: None,
        }
    }

    #[test]
    fn test_iter() {
        let _ = env_logger::try_init();
        let genesis = genesis_block(Network::Testnet);
        let mut current = genesis.clone();
        for b in iter(test_conf()).skip(1) {
            assert_eq!(current.block_hash(), b.block.header.prev_blockhash);
            current = b.block;
        }
        assert_ne!(genesis, current);
    }

    #[test]
    fn test_start_stop() {
        let _ = env_logger::try_init();
        let mut conf = test_conf();
        conf.start_at_height = 2;
        conf.stop_at_height = Some(10);

        let mut iter = iter(conf);

        assert_eq!(
            "000000006c02c8ea6e4ff69651f7fcde348fb9d557a06e6957b65552002a7820",
            iter.next().unwrap().block_hash.to_string()
        );

        assert_eq!(
            "00000000700e92a916b46b8b91a14d1303d5d91ef0b09eecc3151fb958fd9a2e",
            iter.last().unwrap().block_hash.to_string()
        );
    }
}

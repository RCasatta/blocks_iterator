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

#[cfg(test)]
mod inner_test {
    use bitcoin::blockdata::constants::genesis_block;

    use super::*;
    use crate::bitcoin::Network;
    use crate::inner_test::test_conf;
    use test_log::test;

    #[test]
    fn test_iter() {
        let genesis = genesis_block(Network::Testnet);
        let mut current = genesis.clone();
        for b in iter(test_conf()).skip(1) {
            let block = b.block();
            assert_eq!(current.block_hash(), block.header.prev_blockhash);
            current = block;
        }
        assert_ne!(genesis, current);
    }

    #[test]
    fn test_start_stop() {
        let mut conf = test_conf();
        conf.start_at_height = 2;
        conf.stop_at_height = Some(10);

        for _ in 0..2 {
            let mut iter = iter(conf.clone());

            assert_eq!(
                "000000006c02c8ea6e4ff69651f7fcde348fb9d557a06e6957b65552002a7820",
                iter.next().unwrap().block_hash.to_string()
            );

            assert_eq!(
                "00000000700e92a916b46b8b91a14d1303d5d91ef0b09eecc3151fb958fd9a2e",
                iter.last().unwrap().block_hash.to_string()
            );

            conf.skip_prevout = true;
        }
    }
}

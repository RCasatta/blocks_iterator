use crate::{BlockExtra, FsBlock};
use log::info;
use std::convert::TryInto;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Parse {
    receiver: Receiver<Option<FsBlock>>,
    sender: SyncSender<Option<BlockExtra>>,
}

impl Parse {
    pub fn new(
        receiver: Receiver<Option<FsBlock>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Parse {
        Parse { sender, receiver }
    }

    pub fn start(&mut self) {
        let mut busy_time = 0u128;
        let mut height = 0u32;
        let mut now = Instant::now();
        loop {
            busy_time += now.elapsed().as_nanos();
            let received = self.receiver.recv().expect("cannot receive fs block");
            now = Instant::now();

            match received {
                Some(fs_block) => {
                    let mut block_extra: BlockExtra =
                        fs_block.try_into().expect("should find the file");
                    block_extra.height = height;
                    height += 1;
                    busy_time += now.elapsed().as_nanos();
                    self.sender
                        .send(Some(block_extra))
                        .expect("parse cannot send");
                    now = Instant::now();
                }
                None => break,
            }
        }

        info!(
            "ending parse, busy time: {}s, last height: {}",
            busy_time / 1_000_000_000,
            height
        );
        self.sender.send(None).expect("reorder cannot send none");
    }
}

use log::info;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Read {
    blocks_dir: PathBuf,
    sender: SyncSender<Option<Vec<u8>>>,
}

impl Read {
    pub fn new(blocks_dir: PathBuf, sender: SyncSender<Option<Vec<u8>>>) -> Self {
        Read { blocks_dir, sender }
    }

    pub fn start(&mut self) {
        let mut path = self.blocks_dir.clone();
        path.push("blk*.dat");
        info!("listing block files at {:?}", path);
        let mut paths: Vec<PathBuf> = glob::glob(path.to_str().unwrap())
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        paths.sort();
        info!("There are {} block files", paths.len());
        let mut busy_time = 0u128;
        for path in paths.iter() {
            let now = Instant::now();
            let blob = fs::read(path).unwrap_or_else(|_| panic!("failed to read {:?}", path));
            let len = blob.len();
            info!("read {} of {:?}", len, path);
            busy_time = busy_time + now.elapsed().as_nanos();
            self.sender.send(Some(blob)).expect("cannot send");
        }
        self.sender.send(None).expect("cannot send");
        info!("ending reader, busy time: {}s", (busy_time / 1_000_000_000));
    }
}

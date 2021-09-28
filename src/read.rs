use log::{debug, info};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Read {
    blocks_dir: PathBuf,
    sender: SyncSender<Option<PathWithContent>>,
}

pub struct PathWithContent {
    pub path: PathBuf,
    pub content: Vec<u8>,
}

impl Read {
    pub fn new(blocks_dir: PathBuf, sender: SyncSender<Option<PathWithContent>>) -> Self {
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
        for path in paths {
            let now = Instant::now();
            let content = fs::read(&path).unwrap_or_else(|_| panic!("failed to read {:?}", path));
            let len = content.len();
            debug!("read {} of {:?}", len, &path);
            busy_time += now.elapsed().as_nanos();
            self.sender
                .send(Some(PathWithContent { path, content }))
                .expect("cannot send");
        }
        self.sender.send(None).expect("cannot send");
        info!("ending reader, busy time: {}s", (busy_time / 1_000_000_000));
    }
}

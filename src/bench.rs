use bitcoin::Block;
use std::fmt;
use std::fmt::Formatter;
use std::time::Duration;
use std::time::Instant;

/// Contains counter and instants to provide per period stats over transaction and blocks processed
#[derive(Debug)]
pub struct PeriodCounter {
    start: Instant,
    last: Instant,
    stats: Stats,
    period: Duration,
}

#[derive(Debug, Default, Clone)]
pub struct Stats {
    current: BlocksTxs,
    total: BlocksTxs,
}

#[derive(Debug, Default, Clone)]
struct BlocksTxs {
    blocks: u64,
    txs: u64,
    period: Duration,
}

impl BlocksTxs {
    fn blocks(&self) -> u64 {
        ((self.blocks as u128 * 1000u128) / self.period.as_millis()) as u64
    }
    fn txs(&self) -> u64 {
        ((self.txs as u128 * 1000u128) / self.period.as_millis()) as u64
    }
}

impl PeriodCounter {
    /// Create a [`PeriodCounter`] with given `period`
    pub fn new(period: Duration) -> Self {
        PeriodCounter {
            start: Instant::now(),
            last: Instant::now(),
            stats: Default::default(),
            period,
        }
    }

    /// Count statistics of the given block
    pub fn count_block(&mut self, block: &Block) {
        self.stats.current.blocks += 1;
        self.stats.current.txs += block.txdata.len() as u64;

        self.stats.total.blocks += 1;
        self.stats.total.txs += block.txdata.len() as u64;
    }

    /// If `self.period` has passed since last invocation return stats
    pub fn period_elapsed(&mut self) -> Option<Stats> {
        if self.last.elapsed() >= self.period {
            self.stats.total.period = self.start.elapsed();
            self.stats.current.period = self.last.elapsed();
            let return_value = self.stats.clone();
            self.stats.current = BlocksTxs::default();
            self.last = Instant::now();
            Some(return_value)
        } else {
            None
        }
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Current: {:>5} blk/s; {:>6} txs/s; Total: {:>5} blk/s; {:>6} tx/s;",
            self.current.blocks(),
            self.current.txs(),
            self.total.blocks(),
            self.total.txs()
        )
    }
}

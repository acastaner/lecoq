//! Siblings (spec §3.2, §3.3): directory scan, per-platform ordering comparators, the `notify`
//! watcher, prefetch scheduling and the display-size cache.
//!
//! The scan runs only after the first image is painted (§1.3).

pub mod order;
pub mod prefetch;
pub mod scan;
pub mod watch;

pub use prefetch::{Decoded, Prefetcher};
pub use scan::{PendingScan, Scanned, Sibling, SiblingList};
pub use watch::{Change, DirWatcher};

/// Which way the user last moved through the Siblings (§3.3). Forward at startup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}

/// The prefetch window around `index` in a list of `len` Siblings (§3.3): the next two in the
/// direction of travel, then one behind, in priority order.
pub fn prefetch_window(len: usize, index: usize, direction: Direction) -> Vec<usize> {
    let step = |by: isize| {
        let signed = match direction {
            Direction::Forward => by,
            Direction::Backward => -by,
        };
        index.checked_add_signed(signed).filter(|&i| i < len)
    };
    [step(1), step(2), step(-1)].into_iter().flatten().collect()
}

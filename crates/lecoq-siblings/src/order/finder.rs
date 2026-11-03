//! macOS Finder's order: `-[NSString localizedStandardCompare:]` ("Compares strings as sorted by
//! the Finder"), in the current locale.
//!
//! Apple documents that it "may be changed in future releases": calling it is the only way to keep
//! matching Finder, so nothing may be persisted keyed on this order.

use std::cmp::Ordering;

use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{NSComparisonResult, NSString};

use super::NameOrder;

/// Finder's file-name order.
#[derive(Clone, Copy, Debug, Default)]
pub struct FinderOrder;

impl FinderOrder {
    fn compare_ns(a: &NSString, b: &NSString, a_str: &str, b_str: &str) -> Ordering {
        let order = match a.localizedStandardCompare(b) {
            NSComparisonResult::Ascending => Ordering::Less,
            NSComparisonResult::Descending => Ordering::Greater,
            _ => Ordering::Equal,
        };
        // Names Finder ranks equal still get one fixed order.
        order.then_with(|| a_str.cmp(b_str))
    }
}

impl NameOrder for FinderOrder {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        // The scan runs on a thread with no autorelease pool of its own.
        autoreleasepool(|_| Self::compare_ns(&NSString::from_str(a), &NSString::from_str(b), a, b))
    }

    /// Builds each `NSString` once rather than twice per comparison.
    fn sorted_indices(&self, names: &[String]) -> Vec<usize> {
        autoreleasepool(|_| {
            let strings: Vec<Retained<NSString>> =
                names.iter().map(|n| NSString::from_str(n)).collect();
            let mut indices: Vec<usize> = (0..names.len()).collect();
            indices
                .sort_by(|&a, &b| Self::compare_ns(&strings[a], &strings[b], &names[a], &names[b]));
            indices
        })
    }
}

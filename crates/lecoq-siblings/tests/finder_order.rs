//! The macOS Sibling ordering (spec §3.2, §9 "the Sibling ordering comparator"): Finder's
//! `-[NSString localizedStandardCompare:]`.
//!
//! Apple doesn't list the rules; the expected orders below are the documented ones (numeric,
//! case-insensitive: swift-corelibs-foundation builds it from `.numeric` and `.caseInsensitive`).
//! These tables run only on macOS.
#![cfg(target_os = "macos")]

use std::cmp::Ordering;

use lecoq_siblings::order::{FinderOrder, NameOrder};

/// `expected` is the documented order. Sorting it reversed and rotated must give it back, and every
/// pair must compare consistently.
#[track_caller]
fn assert_order(order: &dyn NameOrder, expected: &[&str]) {
    for (i, a) in expected.iter().enumerate() {
        assert_eq!(order.compare(a, a), Ordering::Equal, "{a} = {a}");
        for b in &expected[i + 1..] {
            assert_eq!(order.compare(a, b), Ordering::Less, "{a} < {b}");
            assert_eq!(order.compare(b, a), Ordering::Greater, "{b} > {a}");
        }
    }
    let mut reversed: Vec<String> = expected.iter().rev().map(|s| s.to_string()).collect();
    order.sort(&mut reversed);
    assert_eq!(reversed, expected);
    let mut rotated: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    rotated.rotate_left(expected.len() / 2);
    order.sort(&mut rotated);
    assert_eq!(rotated, expected);
}

#[test]
fn digit_runs_compare_by_value() {
    assert_order(
        &FinderOrder,
        &["img1.png", "img2.png", "img10.png", "img100.png"],
    );
}

#[test]
fn case_is_ignored() {
    // Unlike GLib, case doesn't outrank the number that follows it.
    assert_order(&FinderOrder, &["img3.png", "IMG10.png", "img20.png"]);
    assert_order(&FinderOrder, &["apple.png", "Banana.png", "cherry.png"]);
}

#[test]
fn names_differing_only_in_case_still_have_a_fixed_order() {
    // `.forcedOrdering`: never Equal for different strings.
    let order = FinderOrder;
    assert_ne!(order.compare("IMG3.png", "img3.png"), Ordering::Equal);
    assert_eq!(
        order.compare("IMG3.png", "img3.png"),
        order.compare("img3.png", "IMG3.png").reverse()
    );
}

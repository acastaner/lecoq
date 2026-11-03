//! The Windows Sibling ordering (spec §3.2, §9 "the Sibling ordering comparator"): Explorer's
//! `StrCmpLogicalW`, or an ordinal comparison when the `NoStrCmpLogical` policy is set.
//!
//! Expected orders come from Microsoft's documentation of `StrCmpLogicalW` and of the
//! `NoStrCmpLogical` policy ("111 < 22 < 3"). These tables run only on Windows.
#![cfg(windows)]

use std::cmp::Ordering;

use lecoq_siblings::order::{ExplorerOrder, NameOrder};

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
fn logical_order_is_microsofts_documented_example() {
    assert_order(
        &ExplorerOrder::logical(),
        &[
            "2string", "3string", "20string", "st2ring", "st3ring", "st20ring", "string2",
            "string3", "string20",
        ],
    );
}

#[test]
fn logical_order_compares_digit_runs_by_value() {
    assert_order(
        &ExplorerOrder::logical(),
        &["img1.png", "img2.png", "img10.png", "img100.png"],
    );
}

#[test]
fn logical_order_ignores_case() {
    // Unlike GLib, case doesn't outrank the number that follows it.
    assert_order(
        &ExplorerOrder::logical(),
        &["img3.png", "IMG10.png", "img20.png"],
    );
    assert_order(
        &ExplorerOrder::logical(),
        &["apple.png", "Banana.png", "cherry.png"],
    );
}

#[test]
fn ordinal_order_sorts_digit_by_digit() {
    // The NoStrCmpLogical policy text: "111 < 22 < 3".
    assert_order(&ExplorerOrder::ordinal(), &["111.png", "22.png", "3.png"]);
    assert_order(
        &ExplorerOrder::ordinal(),
        &["img1.png", "img10.png", "img2.png"],
    );
}

#[test]
fn ordinal_order_ignores_case() {
    assert_order(
        &ExplorerOrder::ordinal(),
        &["apple.png", "Banana.png", "cherry.png"],
    );
}

#[test]
fn names_differing_only_in_case_still_have_a_fixed_order() {
    for order in [ExplorerOrder::logical(), ExplorerOrder::ordinal()] {
        assert_ne!(order.compare("IMG3.png", "img3.png"), Ordering::Equal);
        assert_eq!(
            order.compare("IMG3.png", "img3.png"),
            order.compare("img3.png", "IMG3.png").reverse()
        );
    }
}

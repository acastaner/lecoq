//! The GLib Sibling ordering (spec §3.2, §9 "the Sibling ordering comparator"): a port of
//! `g_utf8_collate_key_for_filename`, compared the way Nautilus compares (names starting with `.`
//! or `#` last).
//!
//! Every expected order below was measured with the real GLib 2.88 (`g_utf8_collate_key_for_filename`
//! plus Nautilus's sort-last rule) in the named locale. The locale-dependent tables need the
//! `en_US.UTF-8` locale installed.
//!
//! Linux only: elsewhere `GlibOrder` falls back to code-point order (it is only used on Linux), so
//! these tables would fail in a test run on Windows or macOS.
#![cfg(target_os = "linux")]

use std::cmp::Ordering;

use lecoq_siblings::order::{GlibOrder, NameOrder};

fn en_us() -> GlibOrder {
    GlibOrder::with_locale("en_US.UTF-8")
        .expect("the en_US.UTF-8 locale must be installed to run the ordering tests")
}

fn c_utf8() -> GlibOrder {
    GlibOrder::with_locale("C.UTF-8").expect("C.UTF-8 is built into glibc")
}

/// `expected` is the measured order. Sorting it reversed and rotated must give it back, and every
/// pair must compare consistently.
#[track_caller]
fn assert_order(order: &GlibOrder, expected: &[&str]) {
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
fn numbers_compare_by_value() {
    for order in [en_us(), c_utf8()] {
        assert_order(&order, &["img1.jpg", "img2.jpg", "img10.jpg"]);
        assert_order(
            &order,
            &[
                "IMG_0001.JPG",
                "IMG_0009.JPG",
                "IMG_0010.JPG",
                "IMG_0100.JPG",
            ],
        );
        assert_order(&order, &["x99.png", "x0099.png", "x100.png"]);
        assert_order(
            &order,
            &[
                "Screenshot from 2026-01-02 10-00-00.png",
                "Screenshot from 2026-10-29 09-59-59.png",
                "Screenshot from 2026-10-29 10-05-03.png",
            ],
        );
    }
}

#[test]
fn leading_zeros_only_break_ties_fewest_first() {
    for order in [en_us(), c_utf8()] {
        assert_order(&order, &["img2.jpg", "img02.jpg", "img002.jpg"]);
        assert_order(&order, &["0.png", "00.png", "000.png", "1.png"]);
        assert_order(&order, &["a.png", "a0.png", "a00.png", "a000.png"]);
    }
}

#[test]
fn case_inside_a_text_run_outranks_the_number_after_it() {
    // The GLib quirk: in en_US.UTF-8 lowercase sorts before uppercase within a run, and that
    // difference is decided before the number is reached, so IMG3 comes after img10.
    assert_order(&en_us(), &["img3.png", "img3.PNG", "img10.png", "IMG3.png"]);
    // In C.UTF-8 it's code-point order: uppercase first.
    assert_order(
        &c_utf8(),
        &["IMG3.png", "img3.PNG", "img3.png", "img10.png"],
    );
}

#[test]
fn text_is_collated_in_the_locale() {
    let en = en_us();
    assert_order(&en, &["a.png", "b.png", "B.png", "C.png"]);
    assert_order(&en, &["apple.png", "Apple.png", "zebra.png", "Zebra.png"]);
    assert_order(
        &en,
        &[
            "eclair.png",
            "Éclair.png",
            "resume.png",
            "Resume.png",
            "résumé.png",
        ],
    );
    let c = c_utf8();
    assert_order(&c, &["B.png", "C.png", "a.png", "b.png"]);
    assert_order(
        &c,
        &[
            "Resume.png",
            "eclair.png",
            "resume.png",
            "résumé.png",
            "Éclair.png",
        ],
    );
}

#[test]
fn a_dot_sorts_before_everything() {
    for order in [en_us(), c_utf8()] {
        assert_order(&order, &["a.b.png", "a.png", "a-b.png", "ab.png"]);
        assert_order(&order, &["event.c", "event.h", "eventgenerator.c"]);
        assert_order(&order, &["photo.png", "photo-1.png"]);
        assert_order(&order, &["file.png", "file (1).png"]);
        assert_order(&order, &["photo.jpeg", "photo.jpg", "photo.png"]);
        // Dots separate numbers; there are no decimals.
        assert_order(&order, &["1.5.png", "1.9.png", "1.10.png"]);
    }
}

#[test]
fn names_starting_with_a_dot_or_hash_sort_last() {
    assert_order(
        &en_us(),
        &[
            "apple.png",
            "_under.png",
            "zebra.png",
            ".hidden.png",
            "#note.png",
        ],
    );
    assert_order(
        &c_utf8(),
        &[
            "_under.png",
            "apple.png",
            "zebra.png",
            ".hidden.png",
            "#note.png",
        ],
    );
    for order in [en_us(), c_utf8()] {
        assert_order(&order, &[".a.png", ".b.png", "#a.png", "#b.png"]);
    }
}

#[test]
fn keys_match_glib_byte_for_byte() {
    // Keys of the real g_utf8_collate_key_for_filename in C.UTF-8, where wcsxfrm is the identity.
    let order = c_utf8();
    let hex = |name: &str| {
        order
            .collate_key(name)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    assert_eq!(hex("img1.jpg"), "696d670101010231010101016a7067");
    assert_eq!(hex("img10.png"), "696d67010101023a313001010101706e67");
    assert_eq!(hex("résumé.png"), "72c3a973756dc3a901010101706e67");
}

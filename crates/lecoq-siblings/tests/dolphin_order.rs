//! The Dolphin Sibling ordering (spec §3.2, §9 "the Sibling ordering comparator"): a port of
//! `KFileItemModel`'s natural-sort name comparison, used when `XDG_CURRENT_DESKTOP` contains `KDE`.
//!
//! Every expected order below was measured with Dolphin's own code: the helper namespace and
//! `stringCompare` of `src/kitemviews/kfileitemmodel.cpp` at `e501e8ca` (Dolphin ≥ 26.08) copied
//! verbatim, plus `lessThan`'s name and URL fallbacks, built against Qt 6.10.2 with ICU and run with
//! `QCollator` set up as Dolphin's default `NaturalSorting` sets it (numeric, case-insensitive) in the
//! named locale. ICU carries its own locale data, so none of these need a system locale installed.

use std::cmp::Ordering;

use lecoq_siblings::order::{DolphinOrder, NameOrder, kde_session};

fn en_us() -> DolphinOrder {
    DolphinOrder::with_locale("en_US.UTF-8")
}

fn c() -> DolphinOrder {
    DolphinOrder::with_locale("C")
}

/// `expected` is the measured order. Sorting it reversed and rotated must give it back, and every
/// pair must compare consistently.
#[track_caller]
fn assert_order(order: &DolphinOrder, expected: &[&str]) {
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
fn kde_is_detected_from_any_colon_separated_entry() {
    assert!(kde_session(Some("KDE")));
    assert!(kde_session(Some("ubuntu:KDE")));
    assert!(kde_session(Some("KDE:plasma")));
    // Registered values are case-sensitive, and the whole entry must match.
    assert!(!kde_session(Some("kde")));
    assert!(!kde_session(Some("KDE-Plasma")));
    assert!(!kde_session(Some("ubuntu:GNOME")));
    assert!(!kde_session(Some("")));
    assert!(!kde_session(None));
}

#[test]
fn numbers_compare_by_value() {
    for order in [en_us(), c()] {
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
fn leading_zeros_only_break_ties_more_zeros_first() {
    // Equal values tie, then the literal compare of the base name puts `0` before any other digit.
    for order in [en_us(), c()] {
        assert_order(&order, &["img002.jpg", "img02.jpg", "img2.jpg"]);
        assert_order(&order, &["x0099.png", "x99.png", "x100.png"]);
        assert_order(&order, &["0.png", "00.png", "000.png", "1.png"]);
        assert_order(&order, &["a.png", "a0.png", "a00.png", "a000.png"]);
    }
}

#[test]
fn case_is_ignored_then_breaks_ties_uppercase_first() {
    for order in [en_us(), c()] {
        // Unlike GLib, IMG3 stays next to img3, before img10.
        assert_order(&order, &["IMG3.png", "img3.PNG", "img3.png", "img10.png"]);
        assert_order(&order, &["a.png", "B.png", "b.png", "C.png"]);
        assert_order(
            &order,
            &["Apple.png", "apple.png", "Zebra.png", "zebra.png"],
        );
        assert_order(
            &order,
            &["photo.jpeg", "photo.JPG", "photo.jpg", "photo.png"],
        );
        assert_order(&order, &["Noext", "noext", "NOEXT.jpg", "noext.png"]);
    }
}

#[test]
fn the_base_name_is_compared_before_the_extension() {
    for order in [en_us(), c()] {
        assert_order(&order, &["a.png", "a-b.png", "a.b.png", "ab.png", "B.png"]);
        assert_order(&order, &["photo.png", "photo-1.png"]);
        assert_order(&order, &["file.png", "file (1).png"]);
        assert_order(&order, &["event.c", "event.h", "eventgenerator.c"]);
        assert_order(
            &order,
            &["archive", "archive.tar", "archive.zip", "archive.tar.gz"],
        );
        assert_order(&order, &["a1.png", "a1.b.png", "a1b.png"]);
    }
    assert_order(
        &en_us(),
        &[
            "photo.png",
            "photo (1).png",
            "photo 2.png",
            "photo_1.png",
            "photo-1.png",
            "photo1.png",
        ],
    );
}

#[test]
fn two_dotted_numbers_are_a_decimal() {
    // The 26.08 rule: 1.10 reads as 1.1.
    for order in [en_us(), c()] {
        assert_order(
            &order,
            &["1.png", "1.10.png", "1.5.png", "1.9.png", "2.png"],
        );
        assert_order(
            &order,
            &[
                "2.png",
                "2.05.png",
                "2.5.png",
                "2.50.png",
                "2.500.png",
                "10.png",
            ],
        );
        assert_order(&order, &["x1.png", "x1.0.png", "x1.00.png"]);
    }
}

#[test]
fn three_or_more_dotted_numbers_compare_like_versions() {
    for order in [en_us(), c()] {
        assert_order(
            &order,
            &[
                "v1.png",
                "v1.2.9.png",
                "v1.2.10.png",
                "v1.10.png",
                "v1.9.png",
                "v2.png",
            ],
        );
        assert_order(
            &order,
            &["1.2.png", "1.2.3.png", "1.2.3.4.png", "1.2.3.10.png"],
        );
        assert_order(&order, &["v1.5.png", "v1.5a.png", "v1.5.1.png"]);
    }
}

#[test]
fn punctuation_is_collated_by_icu() {
    assert_order(
        &en_us(),
        &[
            "_under.png",
            "-dash.png",
            ".hidden.png",
            "(paren).png",
            "#note.png",
            "~tilde.png",
            "1.png",
            "apple.png",
            "Zed.png",
        ],
    );
    assert_order(
        &en_us(),
        &["img 8.png", "img_10.png", "img-9.png", "img.7.png"],
    );
}

#[test]
fn text_is_collated_in_the_locale() {
    assert_order(
        &en_us(),
        &[
            "eclair.png",
            "Éclair.png",
            "Resume.png",
            "resume.png",
            "résumé.png",
            "zz.png",
        ],
    );
    assert_order(
        &en_us(),
        &[
            "arg.png",
            "Arger.png",
            "Ärger.png",
            "Ol.png",
            "Öl.png",
            "Zebra.png",
        ],
    );
    // Swedish sorts Ä and Ö after Z.
    assert_order(
        &DolphinOrder::with_locale("sv_SE.UTF-8"),
        &[
            "arg.png",
            "Arger.png",
            "Ol.png",
            "Zebra.png",
            "Ärger.png",
            "Öl.png",
        ],
    );
    // Non-ASCII digits are ICU's business: numeric collation gives them their value.
    assert_order(&en_us(), &["1.png", "١.png", "2.png", "٢٢.png"]);
}

#[test]
fn the_c_locale_compares_code_points_ignoring_case() {
    for name in ["C", "C.UTF-8", "POSIX", ""] {
        let order = DolphinOrder::with_locale(name);
        assert_order(
            &order,
            &[
                "eclair.png",
                "Resume.png",
                "resume.png",
                "résumé.png",
                "zz.png",
                "Éclair.png",
            ],
        );
        assert_order(
            &order,
            &[
                "#note.png",
                "(paren).png",
                "-dash.png",
                ".hidden.png",
                "1.png",
                "_under.png",
                "apple.png",
                "Zed.png",
                "~tilde.png",
            ],
        );
        assert_order(
            &order,
            &["img 8.png", "img-9.png", "img.7.png", "img_10.png"],
        );
        assert_order(&order, &["1.png", "2.png", "١.png", "٢٢.png"]);
    }
}

#[test]
fn a_comparison_cycle_sorts_as_dolphin_sorts_it_in_listing_order() {
    // ICU reads Arabic-Indic digits as numbers but Dolphin's numeric chains only take ASCII ones,
    // so these three form a cycle. Dolphin's merge sort still gives one answer per listing order.
    let order = en_us();
    let (a, b, c) = ("37 x.jpg", "١77.jpg", "1٢38.png");
    assert_eq!(order.compare(a, b), Ordering::Less);
    assert_eq!(order.compare(b, c), Ordering::Less);
    assert_eq!(order.compare(c, a), Ordering::Less);

    let sorted = |listing: &[&str]| {
        let mut names: Vec<String> = listing.iter().map(|s| s.to_string()).collect();
        order.sort(&mut names);
        names
    };
    assert_eq!(sorted(&[a, b, c]), [b, c, a]);
    assert_eq!(sorted(&[c, b, a]), [a, b, c]);
    assert_eq!(sorted(&[b, a, c, "a.png"]), [a, b, c, "a.png"]);
}

#[test]
fn the_codeset_and_modifier_of_the_locale_name_are_ignored() {
    for name in ["sv_SE", "sv_SE.UTF-8", "sv_SE.ISO-8859-1", "sv_SE@euro"] {
        assert_order(
            &DolphinOrder::with_locale(name),
            &["a.png", "z.png", "ä.png"],
        );
    }
    assert_order(
        &DolphinOrder::with_locale("de_DE.UTF-8"),
        &["a.png", "ä.png", "z.png"],
    );
}

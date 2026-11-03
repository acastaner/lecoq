//! A port of Dolphin's name comparison: `KFileItemModel::stringCompare`'s natural-sort branch and
//! the helpers it calls (`src/kitemviews/kfileitemmodel.cpp` at `e501e8ca`, Dolphin ≥ 26.08), with
//! `QCollator` set up as Dolphin's default `NaturalSorting` choice sets it: numeric, case-insensitive.
//!
//! Ported faithfully, quirks included (research/os-filename-ordering.md):
//! - The base name is compared before the extension (the last `.`, unless it sits between two
//!   digits), so `photo.png < photo-1.png` and `a.png < a.b.png`.
//! - ASCII digit runs compare by value. A chain of exactly two dotted runs is a decimal
//!   (`1.10 < 1.5`, the 26.08 rule); three or more compare run by run, like versions.
//! - Leading zeros and case only break ties, through a literal compare of the whole base name:
//!   `img002 < img02 < img2`, `IMG3 < img3`. Names with no digits that tie fall through to
//!   `lessThan`'s URL compare, so case decides there too (`Resume < resume`).
//!
//! Dolphin's `QCollator()` follows `LC_ALL`, then `LC_COLLATE`, then `LANG`. ICU (here ICU4X, with
//! compiled-in CLDR data) does the text comparison; in the C or POSIX locale `QCollator` has no ICU
//! collator and compares UTF-16 case-insensitively instead.
//!
//! The comparison is not a total order (see `sorted_indices`), so sorting uses Dolphin's own merge
//! sort rather than the standard library's.
//!
//! Checked against Dolphin's code built with Qt 6.10 (ICU4C): identical orders for thousands of
//! random names in 13 locales. Known differences: in locales with a tailoring (sv, da, es, pl, ja)
//! ICU4C doesn't read non-ASCII digits (`١٢`) as numbers and ICU4X does; the URL tie-break is a
//! UTF-16 compare of the names (the percent-encoding of `%`, `#` and `?` can't reorder names that
//! differ only in case); and a language Qt doesn't know falls back to the C locale in Qt but to
//! ICU's root collation here.

use std::cmp::Ordering;

use icu_collator::options::{AlternateHandling, CollatorOptions, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::Locale;

use super::NameOrder;

/// The KDE/Dolphin file-manager order, in one locale.
pub struct DolphinOrder {
    text: TextCollator,
}

/// `QCollator` with numeric mode on and case sensitivity off.
enum TextCollator {
    /// ICU at secondary strength (case ignored), punctuation not ignorable, digits by value.
    Icu(CollatorBorrowed<'static>),
    /// The C locale: `QString::compare(.., Qt::CaseInsensitive)`.
    C,
}

impl DolphinOrder {
    /// The order in the user's locale, as Dolphin's `QCollator()` picks it: `LC_ALL`, then
    /// `LC_COLLATE`, then `LANG`.
    pub fn from_environment() -> Self {
        let name = ["LC_ALL", "LC_COLLATE", "LANG"]
            .into_iter()
            .filter_map(|var| std::env::var(var).ok())
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        Self::with_locale(&name)
    }

    /// The order in a POSIX locale name (`"en_US.UTF-8"`, `"sv_SE"`, `"C"`). The codeset and
    /// modifier are ignored, as Qt ignores them; a name that isn't a locale means the C locale.
    pub fn with_locale(name: &str) -> Self {
        Self {
            text: TextCollator::new(name),
        }
    }

    /// `KFileItemModel::stringCompare` (natural branch): base names, then extensions.
    fn string_compare(&self, a: &str, b: &str) -> Ordering {
        let separator_a = find_extension_separator(a);
        let separator_b = find_extension_separator(b);
        let base_a = separator_a.unwrap_or(a.len());
        let base_b = separator_b.unwrap_or(b.len());

        let result = self.decimal_aware_natural_compare(&a[..base_a], &b[..base_b]);
        if result != Ordering::Equal || (separator_a.is_none() && separator_b.is_none()) {
            return result;
        }
        self.decimal_aware_natural_compare(&a[base_a..], &b[base_b..])
    }

    /// `decimalAwareNaturalCompare`. Every structural character is ASCII, so byte indices into the
    /// UTF-8 text stay on character boundaries exactly where Dolphin's UTF-16 indices do.
    fn decimal_aware_natural_compare(&self, a: &str, b: &str) -> Ordering {
        let (bytes_a, bytes_b) = (a.as_bytes(), b.as_bytes());
        let mut compared_numeric_tokens = false;
        let (mut index_a, mut index_b) = (0, 0);

        while index_a < a.len() && index_b < b.len() {
            if bytes_a[index_a].is_ascii_digit() && bytes_b[index_b].is_ascii_digit() {
                compared_numeric_tokens = true;
                let chain_a = NumericChain::at(bytes_a, index_a);
                let chain_b = NumericChain::at(bytes_b, index_b);
                let result = compare_numeric_chains(bytes_a, &chain_a, bytes_b, &chain_b);
                index_a = chain_a.end;
                index_b = chain_b.end;
                if result != Ordering::Equal {
                    return result;
                }
                continue;
            }

            let text_end_a = text_run_end(bytes_a, index_a);
            let text_end_b = text_run_end(bytes_b, index_b);
            if self
                .text
                .compare(&a[index_a..text_end_a], &b[index_b..text_end_b])
                != Ordering::Equal
            {
                // Dolphin then compares the whole remainders, not just the two runs.
                return self.text.compare(&a[index_a..], &b[index_b..]);
            }
            index_a = text_end_a;
            index_b = text_end_b;
        }

        let remainder = self.text.compare(&a[index_a..], &b[index_b..]);
        if remainder != Ordering::Equal || !compared_numeric_tokens {
            return remainder;
        }
        compare_case_insensitive(a, b).then_with(|| compare_utf16(a, b))
    }
}

impl NameOrder for DolphinOrder {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        // `lessThan`'s fallbacks for two local files in one folder: the text, the name (the same
        // here), then the URL, compared case-sensitively.
        self.string_compare(a, b).then_with(|| compare_utf16(a, b))
    }

    /// Dolphin's own merge sort. The comparison is not a total order (ICU reads non-ASCII digits as
    /// numbers, the numeric chains only ASCII ones, so `"37 x" < "١77" < "1٢38"` yet `"37 x" >
    /// "1٢38"`), and the standard library's sort may panic on that. Dolphin's sort never fails and,
    /// given the names in the same order (both list the folder with `readdir`), gives Dolphin's result.
    fn sorted_indices(&self, names: &[String]) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..names.len()).collect();
        let less = |a: usize, b: usize| self.compare(&names[a], &names[b]) == Ordering::Less;
        merge_sort(&mut indices, &less);
        indices
    }
}

/// `mergeSort` in `src/kitemviews/private/kfileitemmodelsortalgorithm.h` (from Qt's
/// `qStableSortHelper`). `parallelMergeSort` splits at the same midpoints, so the result is the same.
fn merge_sort(items: &mut [usize], less: &impl Fn(usize, usize) -> bool) {
    let span = items.len();
    if span < 2 {
        return;
    }
    let middle = span / 2;
    merge_sort(&mut items[..middle], less);
    merge_sort(&mut items[middle..], less);
    merge(items, middle, less);
}

/// `merge` in the same file (from Qt's `qMerge`): in-place, by rotations.
fn merge(items: &mut [usize], pivot: usize, less: &impl Fn(usize, usize) -> bool) {
    let len1 = pivot;
    let len2 = items.len() - pivot;
    if len1 == 0 || len2 == 0 {
        return;
    }
    if len1 + len2 == 2 {
        if less(items[1], items[0]) {
            items.swap(0, 1);
        }
        return;
    }

    let (first_cut, second_cut, len2_half);
    if len1 > len2 {
        first_cut = len1 / 2;
        let value = items[first_cut];
        second_cut = pivot + lower_bound(&items[pivot..], |e| less(e, value));
        len2_half = second_cut - pivot;
    } else {
        len2_half = len2 / 2;
        second_cut = pivot + len2_half;
        let value = items[second_cut];
        first_cut = upper_bound(&items[..pivot], |e| less(value, e));
    }

    items[first_cut..second_cut].rotate_left(pivot - first_cut);
    let new_pivot = first_cut + len2_half;
    merge(&mut items[..new_pivot], first_cut, less);
    merge(&mut items[new_pivot..], second_cut - new_pivot, less);
}

/// libstdc++'s `std::lower_bound` probing: the first element for which `is_less(element)` is false.
/// The probe order matters when the comparison isn't a total order, so this isn't `partition_point`.
fn lower_bound(items: &[usize], is_less: impl Fn(usize) -> bool) -> usize {
    let (mut first, mut len) = (0, items.len());
    while len > 0 {
        let half = len / 2;
        let middle = first + half;
        if is_less(items[middle]) {
            first = middle + 1;
            len -= half + 1;
        } else {
            len = half;
        }
    }
    first
}

/// libstdc++'s `std::upper_bound` probing: the first element for which `value_is_less(element)`.
fn upper_bound(items: &[usize], value_is_less: impl Fn(usize) -> bool) -> usize {
    let (mut first, mut len) = (0, items.len());
    while len > 0 {
        let half = len / 2;
        let middle = first + half;
        if value_is_less(items[middle]) {
            len = half;
        } else {
            first = middle + 1;
            len -= half + 1;
        }
    }
    first
}

impl TextCollator {
    fn new(posix_name: &str) -> Self {
        // `language_TERRITORY.codeset@modifier`: Qt keeps the language and territory only.
        let name = posix_name.split(['.', '@']).next().unwrap_or_default();
        if matches!(name, "" | "C" | "POSIX") {
            return Self::C;
        }
        let Ok(locale) = Locale::try_from_str(&name.replace('_', "-")) else {
            return Self::C;
        };
        let mut preferences = CollatorPreferences::from(&locale);
        preferences.numeric_ordering = Some(CollationNumericOrdering::True);
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Secondary);
        options.alternate_handling = Some(AlternateHandling::NonIgnorable);
        match Collator::try_new(preferences, options) {
            Ok(collator) => Self::Icu(collator),
            Err(_) => Self::C,
        }
    }

    /// `QCollator::compare`: an empty string sorts first without asking the collator.
    fn compare(&self, a: &str, b: &str) -> Ordering {
        match (a.is_empty(), b.is_empty()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => {}
        }
        match self {
            Self::Icu(collator) => collator.compare(a, b),
            Self::C => compare_case_insensitive(a, b),
        }
    }
}

/// A run of ASCII digits, possibly continued by `.digits` segments (`1.2.10`).
struct NumericChain {
    start: usize,
    end: usize,
    segments: usize,
}

impl NumericChain {
    /// `countNumericChainSegments`.
    fn at(text: &[u8], start: usize) -> Self {
        let mut end = digit_run_end(text, start);
        let mut segments = 1;
        while end + 1 < text.len() && text[end] == b'.' && text[end + 1].is_ascii_digit() {
            end = digit_run_end(text, end + 1);
            segments += 1;
        }
        Self {
            start,
            end,
            segments,
        }
    }
}

/// `compareNumericChains`: two two-segment chains are decimals, anything else compares run by run.
fn compare_numeric_chains(
    a: &[u8],
    chain_a: &NumericChain,
    b: &[u8],
    chain_b: &NumericChain,
) -> Ordering {
    if chain_a.segments == 2 && chain_b.segments == 2 {
        let dot_a = digit_run_end(a, chain_a.start);
        let dot_b = digit_run_end(b, chain_b.start);
        return compare_digit_strings(&a[chain_a.start..dot_a], &b[chain_b.start..dot_b])
            .then_with(|| {
                compare_fractional_digit_strings(
                    &a[dot_a + 1..chain_a.end],
                    &b[dot_b + 1..chain_b.end],
                )
            });
    }

    let (mut segment_a, mut segment_b) = (chain_a.start, chain_b.start);
    loop {
        let segment_end_a = digit_run_end(a, segment_a);
        let segment_end_b = digit_run_end(b, segment_b);
        let result =
            compare_digit_strings(&a[segment_a..segment_end_a], &b[segment_b..segment_end_b]);
        if result != Ordering::Equal {
            return result;
        }
        let next_a = segment_end_a < chain_a.end;
        let next_b = segment_end_b < chain_b.end;
        if !next_a || !next_b {
            return next_a.cmp(&next_b);
        }
        segment_a = segment_end_a + 1;
        segment_b = segment_end_b + 1;
    }
}

/// `compareDigitStrings`: by value, leading zeros ignored.
fn compare_digit_strings(a: &[u8], b: &[u8]) -> Ordering {
    fn significant(digits: &[u8]) -> &[u8] {
        let zeros = digits.iter().take_while(|&&d| d == b'0').count();
        &digits[zeros..]
    }
    let (a, b) = (significant(a), significant(b));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// `compareFractionalDigitStrings`: digit by digit, the shorter padded with zeros.
fn compare_fractional_digit_strings(a: &[u8], b: &[u8]) -> Ordering {
    let length = a.len().max(b.len());
    let digit = |digits: &[u8], i: usize| digits.get(i).copied().unwrap_or(b'0');
    (0..length)
        .map(|i| digit(a, i).cmp(&digit(b, i)))
        .find(|&result| result != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

/// `findDigitRunEnd`.
fn digit_run_end(text: &[u8], start: usize) -> usize {
    start
        + text[start..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
}

/// The end of the text run (no ASCII digit) starting at `start`.
fn text_run_end(text: &[u8], start: usize) -> usize {
    start
        + text[start..]
            .iter()
            .take_while(|b| !b.is_ascii_digit())
            .count()
}

/// `findExtensionSeparator`: the last `.` that isn't between two digits and isn't the first
/// character.
fn find_extension_separator(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    (1..bytes.len()).rev().find(|&i| {
        bytes[i] == b'.'
            && !(bytes[i - 1].is_ascii_digit()
                && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit()))
    })
}

/// `QString::compare(a, b, Qt::CaseSensitive)`: UTF-16 code units.
fn compare_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `QString::compare(a, b, Qt::CaseInsensitive)`: case-folded code points.
fn compare_case_insensitive(a: &str, b: &str) -> Ordering {
    a.chars().map(fold_case).cmp(b.chars().map(fold_case))
}

/// Unicode simple case folding (`QChar::toCaseFolded`): single-character lowercasing, except for
/// the lowercase letters that fold to another one and Cherokee, which folds to uppercase.
fn fold_case(c: char) -> char {
    match c {
        'ς' => 'σ',
        'ϐ' => 'β',
        'ϑ' => 'θ',
        'ϕ' => 'φ',
        'ϖ' => 'π',
        'ϰ' => 'κ',
        'ϱ' => 'ρ',
        'ϵ' => 'ε',
        'ſ' => 's',
        'ẛ' => 'ṡ',
        '\u{1FBE}' => 'ι',
        'ᲀ' => 'в',
        'ᲁ' => 'д',
        'ᲂ' => 'о',
        'ᲃ' => 'с',
        'ᲄ' | 'ᲅ' => 'т',
        'ᲆ' => 'ъ',
        'ᲇ' => 'ѣ',
        'ᲈ' => 'ꙋ',
        // Cherokee: capitals stay, small letters fold to them.
        '\u{13A0}'..='\u{13F5}' => c,
        '\u{13F8}'..='\u{13FD}' => char::from_u32(c as u32 - 8).unwrap_or(c),
        '\u{AB70}'..='\u{ABBF}' => char::from_u32(c as u32 - 0xAB70 + 0x13A0).unwrap_or(c),
        _ => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(folded), None) => folded,
                _ => c,
            }
        }
    }
}

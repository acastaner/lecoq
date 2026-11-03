//! A port of GLib's `g_utf8_collate_key_for_filename` (`glib/gunicollate.c` at `e09400a9`), compared
//! the way Nautilus's `compare_by_display_name` compares (`src/nautilus-file.c` at `7f1e3c6f`):
//! names starting with `.` or `#` last, then the keys byte by byte (`strcmp`).
//!
//! Ported faithfully, quirks included (research/os-filename-ordering.md): case inside a text run
//! outranks the number after it (`img10 < IMG3` in `en_US.UTF-8`), a dot sorts before everything,
//! and leading zeros only break ties (`img2 < img02 < img002`).

use std::cmp::Ordering;

use unicode_normalization::UnicodeNormalization;

use super::NameOrder;
use super::locale::Collator;

/// Prefixed to the keys of dots and digit runs so they sort before any text key.
const COLLATION_SENTINEL: &[u8] = b"\x01\x01\x01";

/// The GNOME/GLib file-manager order, in one locale.
pub struct GlibOrder {
    collator: Collator,
}

impl GlibOrder {
    /// The order in the user's locale (`LC_ALL`, `LC_COLLATE`, `LANG`), as Nautilus sees it. Falls
    /// back to `C.UTF-8`, then `C`, when that locale isn't installed.
    pub fn from_environment() -> Self {
        let collator = ["", "C.UTF-8", "C"]
            .into_iter()
            .find_map(Collator::new)
            .expect("the C locale always exists");
        Self { collator }
    }

    /// The order in a named locale (`"en_US.UTF-8"`); `None` if it isn't installed.
    pub fn with_locale(name: &str) -> Option<Self> {
        Collator::new(name).map(|collator| Self { collator })
    }

    /// `g_utf8_collate_key_for_filename(name, -1)`.
    pub fn collate_key(&self, name: &str) -> Vec<u8> {
        let bytes = name.as_bytes();
        let end = bytes.len();
        let mut result = Vec::with_capacity(end * 2);
        // The leading-zero counts, appended after everything else as the last tie-break.
        let mut append = Vec::new();

        let mut prev = 0;
        let mut p = 0;
        while p < end {
            match bytes[p] {
                b'.' => {
                    if prev != p {
                        self.append_text_key(&mut result, &name[prev..p]);
                    }
                    result.extend_from_slice(COLLATION_SENTINEL);
                    result.push(1);
                    prev = p + 1;
                }
                b'0'..=b'9' => {
                    if prev != p {
                        self.append_text_key(&mut result, &name[prev..p]);
                    }
                    result.extend_from_slice(COLLATION_SENTINEL);
                    result.push(2);
                    prev = p;

                    let (mut leading_zeros, mut digits) = if bytes[p] == b'0' {
                        (1i32, 0i32)
                    } else {
                        (0, 1)
                    };
                    loop {
                        p += 1;
                        if p >= end {
                            break;
                        }
                        if bytes[p] == b'0' && digits == 0 {
                            leading_zeros += 1;
                        } else if bytes[p].is_ascii_digit() {
                            digits += 1;
                        } else {
                            // An all-zero run counts as one digit plus leading zeros.
                            if digits == 0 {
                                digits += 1;
                                leading_zeros -= 1;
                            }
                            break;
                        }
                    }

                    // The number of digits, minus one, as colons: numbers compare by length first.
                    let colons = usize::try_from(digits - 1).unwrap_or(0);
                    result.resize(result.len() + colons, b':');
                    if leading_zeros > 0 {
                        append.push(leading_zeros as u8);
                        prev += leading_zeros as usize;
                    }
                    result.extend_from_slice(&bytes[prev..p]);
                    prev = p;
                    // The outer loop's increment is folded in: `p` already points past the run.
                    continue;
                }
                _ => {}
            }
            p += 1;
        }
        if prev != p {
            self.append_text_key(&mut result, &name[prev..p]);
        }
        // GLib appends `append` as a C string: a count that wrapped to 0 ends it.
        result.extend(append.into_iter().take_while(|&b| b != 0));
        result
    }

    /// `g_string_append (result, g_utf8_collate_key (text))` on a platform with ISO 10646 `wchar_t`:
    /// NFKC, `wcsxfrm`, each weight written as (extended) UTF-8. Appended as a C string, so it stops
    /// at a zero byte.
    fn append_text_key(&self, out: &mut Vec<u8>, text: &str) {
        let normalized: Vec<char> = text.nfkc().collect();
        for weight in self.collator.transform(&normalized) {
            if weight == 0 {
                return;
            }
            utf8_encode(out, weight);
        }
    }
}

/// GLib's `utf8_encode` in `gunicollate.c`: UTF-8, extended to six bytes for weights beyond Unicode.
fn utf8_encode(out: &mut Vec<u8>, val: u32) {
    if val < 0x80 {
        out.push(val as u8);
        return;
    }
    let val = u64::from(val);
    // n bytes carry 5n + 1 bits.
    let mut len = 2;
    while val >= 1 << (5 * len + 1) {
        len += 1;
    }
    let start = out.len();
    out.resize(start + len, 0);
    let buf = &mut out[start..];
    let mut v = val;
    buf[len - 1] = 0x80 | (v & 0x3f) as u8;
    for i in (1..len - 1).rev() {
        v >>= 6;
        buf[i] = 0x80 | (v & 0x3f) as u8;
    }
    v >>= 6;
    buf[0] = ((0xffu64 << (8 - len)) | v) as u8;
}

/// Nautilus puts names starting with `.` or `#` after all others.
fn sorts_last(name: &str) -> bool {
    matches!(name.as_bytes().first(), Some(b'.' | b'#'))
}

impl NameOrder for GlibOrder {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        sorts_last(a)
            .cmp(&sorts_last(b))
            .then_with(|| self.collate_key(a).cmp(&self.collate_key(b)))
            // Equal keys (NFKC twins): any fixed order will do.
            .then_with(|| a.cmp(b))
    }

    fn sorted_indices(&self, names: &[String]) -> Vec<usize> {
        let keys: Vec<(bool, Vec<u8>)> = names
            .iter()
            .map(|name| (sorts_last(name), self.collate_key(name)))
            .collect();
        let mut indices: Vec<usize> = (0..names.len()).collect();
        indices.sort_by(|&a, &b| keys[a].cmp(&keys[b]).then_with(|| names[a].cmp(&names[b])));
        indices
    }
}

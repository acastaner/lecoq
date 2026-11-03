//! Windows Explorer's order: `StrCmpLogicalW`, or an ordinal comparison when the
//! `NoStrCmpLogical` policy ("Turn off numerical sorting in File Explorer") is set.
//!
//! Microsoft documents that `StrCmpLogicalW` "can change from release to release": calling it is
//! the only way to keep matching Explorer, so nothing may be persisted keyed on this order.

use std::cmp::Ordering;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Globalization::CompareStringOrdinal;
use windows::Win32::Globalization::{CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
};
use windows::Win32::UI::Shell::StrCmpLogicalW;
use windows::core::{PCWSTR, w};

use super::NameOrder;

/// Explorer's file-name order.
#[derive(Clone, Copy, Debug)]
pub struct ExplorerOrder {
    logical: bool,
}

impl ExplorerOrder {
    /// The order Explorer uses on this machine: logical unless the `NoStrCmpLogical` policy is set.
    pub fn from_environment() -> Self {
        if no_str_cmp_logical() {
            Self::ordinal()
        } else {
            Self::logical()
        }
    }

    /// `StrCmpLogicalW`: digit runs by value, case-insensitive (Explorer's default).
    pub fn logical() -> Self {
        Self { logical: true }
    }

    /// Ordinal, case-insensitive (Explorer with the `NoStrCmpLogical` policy: "111 < 22 < 3").
    pub fn ordinal() -> Self {
        Self { logical: false }
    }
}

impl NameOrder for ExplorerOrder {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        let a = wide(a);
        let b = wide(b);
        let order = if self.logical {
            // SAFETY: both are NUL-terminated UTF-16 strings that outlive the call.
            let result = unsafe { StrCmpLogicalW(PCWSTR(a.as_ptr()), PCWSTR(b.as_ptr())) };
            result.cmp(&0)
        } else {
            ordinal(&a, &b, true)
        };
        // Names Explorer ranks equal (they differ only in case) still get one fixed order.
        order.then_with(|| ordinal(&a, &b, false))
    }
}

/// `CompareStringOrdinal` over NUL-terminated UTF-16 (the terminator is left out).
fn ordinal(a: &[u16], b: &[u16], ignore_case: bool) -> Ordering {
    let a = &a[..a.len() - 1];
    let b = &b[..b.len() - 1];
    // SAFETY: plain slices; the function takes their lengths.
    match unsafe { CompareStringOrdinal(a, b, ignore_case) } {
        CSTR_LESS_THAN => Ordering::Less,
        CSTR_EQUAL => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        // Only fails on invalid arguments; fall back to code-unit order.
        _ => a.cmp(b),
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Is `NoStrCmpLogical` set? The machine policy wins over the user policy, as Group Policy does.
fn no_str_cmp_logical() -> bool {
    let read = |root: HKEY| -> Option<u32> {
        let mut value: u32 = 0;
        let mut size = size_of::<u32>() as u32;
        // SAFETY: `value` and `size` are valid for the DWORD the flags restrict the read to.
        let status = unsafe {
            RegGetValueW(
                root,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer"),
                w!("NoStrCmpLogical"),
                RRF_RT_REG_DWORD,
                None,
                Some((&raw mut value).cast()),
                Some(&raw mut size),
            )
        };
        (status == ERROR_SUCCESS).then_some(value)
    };
    read(HKEY_LOCAL_MACHINE)
        .or_else(|| read(HKEY_CURRENT_USER))
        .is_some_and(|value| value != 0)
}

//! The libc collation GLib's `g_utf8_collate_key` sits on: `wcsxfrm` in a locale.
//!
//! GLib uses the process locale set by `setlocale(LC_ALL, "")`. lecoq must not touch the process
//! locale from a worker thread, so it opens the same locale as its own `locale_t` with
//! `newlocale(LC_ALL_MASK, "", …)` (which reads `LC_ALL`, `LC_COLLATE` and `LANG` the same way) and
//! calls `wcsxfrm_l`.

/// A collation locale.
pub struct Collator {
    #[cfg(target_os = "linux")]
    locale: libc::locale_t,
}

// SAFETY: a `locale_t` is immutable once created; `wcsxfrm_l` only reads it, from any thread.
#[cfg(target_os = "linux")]
unsafe impl Send for Collator {}
#[cfg(target_os = "linux")]
unsafe impl Sync for Collator {}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn wcsxfrm_l(
        dest: *mut libc::wchar_t,
        src: *const libc::wchar_t,
        n: libc::size_t,
        locale: libc::locale_t,
    ) -> libc::size_t;
}

#[cfg(target_os = "linux")]
impl Collator {
    /// The named locale (`"en_US.UTF-8"`), or the environment's for `""`. `None` if it isn't
    /// installed.
    pub fn new(name: &str) -> Option<Self> {
        let name = std::ffi::CString::new(name).ok()?;
        // SAFETY: a valid C string and no base locale.
        let locale =
            unsafe { libc::newlocale(libc::LC_ALL_MASK, name.as_ptr(), std::ptr::null_mut()) };
        (!locale.is_null()).then_some(Self { locale })
    }

    /// `wcsxfrm` of `text` (UTF-32, no terminator): the collation weights, compared as integers.
    pub fn transform(&self, text: &[char]) -> Vec<u32> {
        let mut src: Vec<libc::wchar_t> = text.iter().map(|&c| c as libc::wchar_t).collect();
        src.push(0);
        // SAFETY: `src` is NUL-terminated; a null `dest` with `n` 0 only measures.
        let len = unsafe { wcsxfrm_l(std::ptr::null_mut(), src.as_ptr(), 0, self.locale) };
        let mut dest: Vec<libc::wchar_t> = vec![0; len + 1];
        // SAFETY: `dest` holds `len + 1` wide characters, the size just measured plus the NUL.
        unsafe { wcsxfrm_l(dest.as_mut_ptr(), src.as_ptr(), len + 1, self.locale) };
        dest.truncate(len);
        dest.into_iter().map(|w| w as u32).collect()
    }
}

#[cfg(target_os = "linux")]
impl Drop for Collator {
    fn drop(&mut self) {
        // SAFETY: created by `newlocale` and freed once.
        unsafe { libc::freelocale(self.locale) };
    }
}

/// Elsewhere the GLib order is only a fallback (#17 replaces it): code-point order, as in the C
/// locale.
#[cfg(not(target_os = "linux"))]
impl Collator {
    pub fn new(_name: &str) -> Option<Self> {
        Some(Self {})
    }

    pub fn transform(&self, text: &[char]) -> Vec<u32> {
        text.iter().map(|&c| c as u32).collect()
    }
}

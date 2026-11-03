//! Sibling ordering (spec §3.2): the file manager's own order, reproduced exactly, quirks included.
//!
//! One [`NameOrder`] per ordering rule, picked by [`platform_order`]: [`GlibOrder`] (Nautilus and
//! every non-KDE Linux desktop), [`DolphinOrder`] (KDE), `ExplorerOrder` on Windows and
//! `FinderOrder` on macOS (the OS's own comparisons).

mod dolphin;
#[cfg(windows)]
mod explorer;
#[cfg(target_os = "macos")]
mod finder;
mod glib;
mod locale;

use std::cmp::Ordering;

pub use dolphin::DolphinOrder;
#[cfg(windows)]
pub use explorer::ExplorerOrder;
#[cfg(target_os = "macos")]
pub use finder::FinderOrder;
pub use glib::GlibOrder;

/// How a file manager orders file names.
pub trait NameOrder: Send + Sync {
    /// Compare two file names (not paths).
    fn compare(&self, a: &str, b: &str) -> Ordering;

    /// The indices of `names` in this order. Implementations with a sort-key form override this so
    /// that each name's key is built once.
    fn sorted_indices(&self, names: &[String]) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..names.len()).collect();
        indices.sort_by(|&a, &b| self.compare(&names[a], &names[b]));
        indices
    }

    /// Sort file names into this order.
    fn sort(&self, names: &mut Vec<String>) {
        let indices = self.sorted_indices(names);
        let mut taken: Vec<Option<String>> = names.drain(..).map(Some).collect();
        names.extend(indices.into_iter().filter_map(|i| taken[i].take()));
    }
}

/// The ordering of Explorer: `StrCmpLogicalW`, or ordinal under the `NoStrCmpLogical` policy.
#[cfg(windows)]
pub fn platform_order() -> Box<dyn NameOrder> {
    Box::new(ExplorerOrder::from_environment())
}

/// The ordering of Finder: `localizedStandardCompare:` in the user's locale.
#[cfg(target_os = "macos")]
pub fn platform_order() -> Box<dyn NameOrder> {
    Box::new(FinderOrder)
}

/// The ordering of the file manager on this desktop, in the user's locale.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn platform_order() -> Box<dyn NameOrder> {
    if cfg!(target_os = "linux")
        && kde_session(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref())
    {
        return Box::new(DolphinOrder::from_environment());
    }
    Box::new(GlibOrder::from_environment())
}

/// Whether `XDG_CURRENT_DESKTOP` names a KDE session: one of its colon-separated entries is exactly
/// `KDE` (registered values are case-sensitive).
pub fn kde_session(xdg_current_desktop: Option<&str>) -> bool {
    xdg_current_desktop.is_some_and(|desktops| desktops.split(':').any(|entry| entry == "KDE"))
}

//! `SiblingList::locate` answers "where is this path" (spec §3.2), and the status bar asks it
//! every frame (#62). Whatever it does inside, the index it returns must agree with a plain scan
//! of the list, and must keep agreeing after the list gains or loses a file. These tests pin that
//! contract through the public interface, with an independent linear scan as the source of truth.

use std::cmp::Ordering;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use lecoq_siblings::order::NameOrder;
use lecoq_siblings::scan::scan;
use lecoq_siblings::{Change, SiblingList};
use tempfile::TempDir;

/// A plain byte order, so the tests don't depend on any installed locale.
struct ByteOrder;
impl NameOrder for ByteOrder {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        a.cmp(b)
    }
}

/// Scan a fresh directory of `names` (empty `.jpg` files) into a [`SiblingList`].
fn list_of(names: &[&str]) -> (TempDir, SiblingList) {
    let dir = TempDir::new().unwrap();
    for name in names {
        File::create(dir.path().join(name)).unwrap();
    }
    let list = scan(dir.path(), Arc::new(ByteOrder)).unwrap();
    (dir, list)
}

/// Where `path` sits by walking the list one Sibling at a time: the answer `locate` must match for
/// a file that is in the list.
fn linear_locate(list: &SiblingList, path: &Path) -> Option<usize> {
    (0..list.len()).find(|&i| list.get(i).unwrap().path == path)
}

#[test]
fn locate_agrees_with_a_linear_scan_of_the_whole_list() {
    let (dir, list) = list_of(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    for name in ["a.jpg", "b.jpg", "c.jpg", "d.jpg"] {
        let path = dir.path().join(name);
        assert_eq!(Ok(linear_locate(&list, &path).unwrap()), list.locate(&path));
    }
}

#[test]
fn a_file_that_is_not_a_sibling_locates_at_its_insertion_point() {
    let (dir, list) = list_of(&["a.jpg", "c.jpg", "e.jpg"]);
    // Between c and e, so it would be inserted at index 2.
    assert_eq!(list.locate(&dir.path().join("d.jpg")), Err(2));
    // Before everything, and after everything.
    assert_eq!(list.locate(&dir.path().join("0.jpg")), Err(0));
    assert_eq!(list.locate(&dir.path().join("z.jpg")), Err(3));
}

#[test]
fn remove_renumbers_the_following_siblings_and_drops_the_removed_one() {
    let (dir, mut list) = list_of(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    let (a, b, c, d) = (
        dir.path().join("a.jpg"),
        dir.path().join("b.jpg"),
        dir.path().join("c.jpg"),
        dir.path().join("d.jpg"),
    );
    assert_eq!(list.locate(&b), Ok(1));

    assert!(list.remove(&b));

    // The removed file is gone; a cache that forgot to invalidate would still report Ok(1).
    assert_eq!(list.locate(&b), Err(1));
    // The survivors keep agreeing with a plain scan: a shifts stays, c and d shift down.
    assert_eq!(list.locate(&a), Ok(linear_locate(&list, &a).unwrap()));
    assert_eq!(list.locate(&c), Ok(linear_locate(&list, &c).unwrap()));
    assert_eq!(list.locate(&d), Ok(linear_locate(&list, &d).unwrap()));
    assert_eq!(list.locate(&c), Ok(1));
    assert_eq!(list.locate(&d), Ok(2));
}

#[test]
fn remove_of_a_file_that_is_not_there_changes_nothing() {
    let (dir, mut list) = list_of(&["a.jpg", "b.jpg"]);
    assert!(!list.remove(&dir.path().join("gone.jpg")));
    assert_eq!(list.locate(&dir.path().join("a.jpg")), Ok(0));
    assert_eq!(list.locate(&dir.path().join("b.jpg")), Ok(1));
}

#[test]
fn apply_added_inserts_in_order_and_renumbers_the_rest() {
    let (dir, mut list) = list_of(&["a.jpg", "c.jpg"]);
    let b = dir.path().join("b.jpg");
    File::create(&b).unwrap();

    assert!(list.apply(&Change::Added(b.clone())));

    // The new file is locatable where a plain scan finds it, and c has shifted to make room.
    assert_eq!(list.locate(&b), Ok(linear_locate(&list, &b).unwrap()));
    assert_eq!(list.locate(&b), Ok(1));
    assert_eq!(list.locate(&dir.path().join("a.jpg")), Ok(0));
    assert_eq!(list.locate(&dir.path().join("c.jpg")), Ok(2));
}

#[test]
fn apply_removed_renumbers_like_remove() {
    let (dir, mut list) = list_of(&["a.jpg", "b.jpg", "c.jpg"]);
    let b = dir.path().join("b.jpg");

    assert!(list.apply(&Change::Removed(b.clone())));

    assert_eq!(list.locate(&b), Err(1));
    assert_eq!(list.locate(&dir.path().join("c.jpg")), Ok(1));
}

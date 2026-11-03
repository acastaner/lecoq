//! Portable mode (#52): a `portable.txt` next to the executable keeps `config.toml` in a `config/`
//! folder beside it, so lecoq can run from any folder (a USB stick, say) and take its settings along.

use lecoq_platform::{ConfigStore, OsConfigStore};

#[test]
fn with_portable_txt_the_config_lives_next_to_the_executable() {
    let exe_dir = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();
    std::fs::write(exe_dir.path().join("portable.txt"), "").unwrap();

    let store = OsConfigStore::locate(Some(exe_dir.path()), Some(base.path()));

    assert_eq!(
        store.path(),
        Some(exe_dir.path().join("config/config.toml").as_path())
    );
}

#[test]
fn in_portable_mode_a_zimged_config_is_not_copied() {
    let exe_dir = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();
    std::fs::write(exe_dir.path().join("portable.txt"), "").unwrap();
    std::fs::create_dir(base.path().join("zimged")).unwrap();
    std::fs::write(base.path().join("zimged/config.toml"), "version = 1\n").unwrap();

    let store = OsConfigStore::locate(Some(exe_dir.path()), Some(base.path()));

    assert_eq!(store.read().unwrap(), None);
    assert!(!base.path().join("lecoq").exists());
}

#[test]
fn without_portable_txt_the_config_lives_in_the_config_directory() {
    let exe_dir = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();

    let store = OsConfigStore::locate(Some(exe_dir.path()), Some(base.path()));

    assert_eq!(
        store.path(),
        Some(base.path().join("lecoq/config.toml").as_path())
    );
}

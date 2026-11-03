//! The one-time copy of zimged's config.toml into lecoq's folder (#51): lecoq was called zimged up to
//! v0.1.6, so a user upgrading keeps their settings.

use lecoq_platform::{ConfigStore, OsConfigStore};

const ZIMGED_CONFIG: &str = "version = 1\njpeg_quality = 70\n";

#[test]
fn a_zimged_config_is_copied_when_lecoq_has_none() {
    let base = tempfile::tempdir().unwrap();
    std::fs::create_dir(base.path().join("zimged")).unwrap();
    std::fs::write(base.path().join("zimged/config.toml"), ZIMGED_CONFIG).unwrap();

    let store = OsConfigStore::in_config_dir(base.path());

    assert_eq!(store.read().unwrap().as_deref(), Some(ZIMGED_CONFIG));
    assert_eq!(
        store.path(),
        Some(base.path().join("lecoq/config.toml").as_path())
    );
    assert!(
        base.path().join("zimged/config.toml").exists(),
        "the old file is left in place"
    );
}

#[test]
fn an_existing_lecoq_config_is_never_overwritten() {
    let base = tempfile::tempdir().unwrap();
    for dir in ["zimged", "lecoq"] {
        std::fs::create_dir(base.path().join(dir)).unwrap();
    }
    std::fs::write(base.path().join("zimged/config.toml"), ZIMGED_CONFIG).unwrap();
    std::fs::write(
        base.path().join("lecoq/config.toml"),
        "version = 1\njpeg_quality = 85\n",
    )
    .unwrap();

    let store = OsConfigStore::in_config_dir(base.path());

    assert_eq!(
        store.read().unwrap().as_deref(),
        Some("version = 1\njpeg_quality = 85\n")
    );
}

#[test]
fn without_either_config_nothing_is_created() {
    let base = tempfile::tempdir().unwrap();

    let store = OsConfigStore::in_config_dir(base.path());

    assert_eq!(store.read().unwrap(), None);
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

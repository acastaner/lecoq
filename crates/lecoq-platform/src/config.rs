//! `config.toml` (§8.3): where it is stored, and the settings read from it.
//!
//! The file is TOML with `version = 1` first. Unknown keys are ignored and missing keys take their
//! defaults. A file that isn't valid TOML, or whose known keys have the wrong type, is corrupt: it
//! is set aside as `config.toml.bad` and lecoq starts with the defaults. Writes go through
//! [`ConfigStore::write`], which replaces the file atomically; the app debounces them.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// The `version` written at the top of `config.toml`.
pub const CONFIG_VERSION: u32 = 1;

/// Reads and writes the raw text of `config.toml`.
pub trait ConfigStore: Send + Sync {
    /// The file's text, or `None` when there is no file yet.
    fn read(&self) -> io::Result<Option<String>>;
    /// Replace the file's text atomically: a reader sees the old text or the new, never a mix.
    fn write(&self, text: &str) -> io::Result<()>;
    /// Rename a corrupt `config.toml` to `config.toml.bad`, replacing an older `.bad` file.
    fn set_aside(&self) -> io::Result<()>;
}

/// `config.toml` in the OS's user-config directory, resolved with `directories`:
/// `$XDG_CONFIG_HOME/lecoq/` (default `~/.config/lecoq/`), `%APPDATA%\lecoq\`, or
/// `~/Library/Application Support/lecoq/`; in portable mode, a `config/` folder next to the
/// executable ([`OsConfigStore::locate`]). See [`OsConfigStore::in_config_dir`] for the copy of
/// an older zimged config.
pub struct OsConfigStore {
    path: Option<PathBuf>,
}

impl OsConfigStore {
    pub fn new() -> Self {
        let exe_dir = std::env::current_exe()
            .and_then(std::fs::canonicalize)
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let dirs = directories::BaseDirs::new();
        Self::locate(exe_dir.as_deref(), dirs.as_ref().map(|d| d.config_dir()))
    }

    /// The store for the executable in `exe_dir`, with `config_dir` the OS's user-config directory.
    /// A `portable.txt` next to the executable is portable mode (#52): `config.toml` lives in a
    /// `config/` folder beside it. Otherwise it is [`Self::in_config_dir`].
    pub fn locate(exe_dir: Option<&Path>, config_dir: Option<&Path>) -> Self {
        if let Some(exe_dir) = exe_dir.filter(|dir| dir.join("portable.txt").is_file()) {
            return Self::at(exe_dir.join("config").join("config.toml"));
        }
        match config_dir {
            Some(base) => Self::in_config_dir(base),
            None => Self { path: None },
        }
    }

    /// The store for `lecoq/config.toml` in this user-config directory. lecoq was called zimged up
    /// to v0.1.6: while there is no `lecoq/` folder yet, a `zimged/config.toml` is copied into a
    /// new one. The old file stays; once `lecoq/` exists it is never read again.
    pub fn in_config_dir(base: &Path) -> Self {
        let dir = base.join("lecoq");
        let path = dir.join("config.toml");
        let legacy = base.join("zimged").join("config.toml");
        if !dir.exists() && legacy.is_file() {
            // Copy to a temporary file and rename it in, so a failed copy leaves no partial
            // config.toml; the empty folder is removed again so the next launch retries.
            let tmp = path.with_extension("toml.tmp");
            let copied = std::fs::create_dir_all(&dir)
                .and_then(|()| std::fs::copy(&legacy, &tmp))
                .and_then(|_| std::fs::rename(&tmp, &path));
            if let Err(e) = copied {
                eprintln!("lecoq: can't copy the zimged config.toml: {e}");
                let _ = std::fs::remove_file(&tmp);
                let _ = std::fs::remove_dir(&dir);
            }
        }
        Self::at(path)
    }

    /// A store for the `config.toml` at this path (any directory, e.g. a temporary one in tests).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    /// Where `config.toml` is, if the OS has a user-config directory.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

impl Default for OsConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigStore for OsConfigStore {
    fn read(&self) -> io::Result<Option<String>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&self, text: &str) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Err(io::Error::other("no config directory"));
        };
        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        // Write a temporary file next to it, flush it to disk, then rename it over the old one.
        let tmp = path.with_extension("toml.tmp");
        let result = (|| {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    fn set_aside(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        std::fs::rename(path, path.with_extension("toml.bad"))
    }
}

/// An in-memory `config.toml`, for tests.
#[derive(Default)]
pub struct MemoryConfigStore {
    text: Mutex<Option<String>>,
    bad: Mutex<Option<String>>,
    writes: Mutex<usize>,
}

impl MemoryConfigStore {
    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            text: Mutex::new(Some(text.into())),
            ..Self::default()
        }
    }

    /// What was last written (or the initial text); `None` when there is no file.
    pub fn text(&self) -> Option<String> {
        self.text.lock().unwrap().clone()
    }

    /// The text of `config.toml.bad`, if a corrupt file was set aside.
    pub fn bad_text(&self) -> Option<String> {
        self.bad.lock().unwrap().clone()
    }

    /// How many times the file was written.
    pub fn write_count(&self) -> usize {
        *self.writes.lock().unwrap()
    }
}

impl ConfigStore for MemoryConfigStore {
    fn read(&self) -> io::Result<Option<String>> {
        Ok(self.text())
    }

    fn write(&self, text: &str) -> io::Result<()> {
        *self.text.lock().unwrap() = Some(text.to_owned());
        *self.writes.lock().unwrap() += 1;
        Ok(())
    }

    fn set_aside(&self) -> io::Result<()> {
        let text = self.text.lock().unwrap().take();
        if text.is_some() {
            *self.bad.lock().unwrap() = text;
        }
        Ok(())
    }
}

/// The remembered window geometry, in logical points. `width` and `height` are the restored
/// (not maximised) size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub width: f32,
    pub height: f32,
    pub maximised: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1600.0,
            height: 1000.0,
            maximised: false,
        }
    }
}

/// Settings from `config.toml`, with the §8.3 defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub allow_multiple_instances: bool,
    /// View → *Follow files after copy/move* (§8.3, issue #53): after a Copy to or Move to, show
    /// the file at its new location rather than staying put. Off by default; tolerant of its
    /// absence in a file written before it existed.
    pub follow_files: bool,
    /// 1–100. Read as any TOML integer and clamped, so an out-of-range value isn't corrupt.
    #[serde(deserialize_with = "jpeg_quality_clamped")]
    pub jpeg_quality: u8,
    pub magnify_factor: f32,
    // Tables come after the plain keys in TOML.
    pub window: WindowConfig,
    /// `tools.<tool>`: each tool's last style (§4.3), keyed by the tool's config name
    /// (`rectangle`, `step_counter`, …). Kept as raw tables: the editor reads them as styles, and
    /// one it can't read falls back to that tool's defaults without making the file corrupt.
    pub tools: BTreeMap<String, toml::Table>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allow_multiple_instances: true,
            follow_files: false,
            jpeg_quality: 92,
            magnify_factor: 2.0,
            window: WindowConfig::default(),
            tools: BTreeMap::new(),
        }
    }
}

/// The file as written: `version` first, then the settings.
#[derive(Serialize)]
struct ConfigFile<'a> {
    version: u32,
    #[serde(flatten)]
    config: &'a Config,
}

/// The JPEG quality range (§8.3).
const JPEG_QUALITY: std::ops::RangeInclusive<i64> = 1..=100;

fn jpeg_quality_clamped<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let quality = i64::deserialize(d)?.clamp(*JPEG_QUALITY.start(), *JPEG_QUALITY.end());
    Ok(u8::try_from(quality).expect("1..=100 fits in u8"))
}

/// The smallest window size kept from the file, in points.
const MIN_WINDOW_SIDE: f32 = 100.0;

impl Config {
    /// Read the config. Never fails: a missing or unreadable file gives the defaults, and a corrupt
    /// one is set aside as `config.toml.bad` first.
    pub fn load(store: &dyn ConfigStore) -> Self {
        let text = match store.read() {
            Ok(Some(text)) => text,
            Ok(None) => return Self::default(),
            Err(e) => {
                eprintln!("lecoq: can't read config.toml, using the defaults: {e}");
                return Self::default();
            }
        };
        match Self::parse(&text) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("lecoq: config.toml is corrupt, renamed to config.toml.bad: {e}");
                if let Err(e) = store.set_aside() {
                    eprintln!("lecoq: can't rename config.toml to config.toml.bad: {e}");
                }
                Self::default()
            }
        }
    }

    /// Parse the text of `config.toml`. Out-of-range values are brought back into range.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let config: Self = toml::from_str(text)?;
        Ok(config.sanitised())
    }

    /// The text of `config.toml` for this config, starting with `version = 1`.
    pub fn to_toml(&self) -> String {
        toml::to_string(&ConfigFile {
            version: CONFIG_VERSION,
            config: self,
        })
        .expect("Config always serialises")
    }

    fn sanitised(mut self) -> Self {
        let defaults = Self::default();
        if !(self.magnify_factor.is_finite() && self.magnify_factor >= 1.0) {
            self.magnify_factor = defaults.magnify_factor;
        }
        let side_ok = |side: f32| side.is_finite() && side >= MIN_WINDOW_SIDE;
        if !(side_ok(self.window.width) && side_ok(self.window.height)) {
            self.window.width = defaults.window.width;
            self.window.height = defaults.window.height;
        }
        self
    }
}

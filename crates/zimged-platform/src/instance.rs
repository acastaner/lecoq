//! Single instance (§8.4): with *Allow multiple instances* off, a starting zimged hands its path to
//! the running one over a local socket and exits; if nothing answers, it becomes the listener.
//!
//! [`SingleInstance`] is both halves: startup calls [`SingleInstance::hand_off_or_listen`] once,
//! before the window exists; the running app drains [`SingleInstance::take_arrivals`] each frame.
//! [`LocalSocketInstance`] is the real one (`interprocess`: a Unix socket in `$XDG_RUNTIME_DIR`, a
//! named pipe on Windows); [`MemoryInstance`] is the fake for the headless App tests.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ConnectOptions, ListenerOptions, Name};

/// Another zimged process reached this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arrival {
    /// It was started with this file (absolute): raise the window and open it.
    Open(PathBuf),
    /// It was started without a file: just raise the window.
    Raise,
}

/// What [`SingleInstance::hand_off_or_listen`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandOff {
    /// A running instance took the path: this process exits.
    Answered,
    /// Nothing answered: this process now listens for later ones.
    Listening,
    /// Nothing answered and listening failed (e.g. a hung instance holds the socket): run alone.
    Alone,
}

/// The single-instance socket, both halves.
pub trait SingleInstance: Send + Sync {
    /// Startup (§1.3 step 2): offer `path` to a running instance. If none answers, become the
    /// listener. Blocks for at most a short timeout.
    fn hand_off_or_listen(&self, path: Option<&Path>) -> HandOff;
    /// Called when an [`Arrival`] lands (from the listener's thread): the app wakes its UI here.
    fn set_wake(&self, wake: Box<dyn Fn() + Send + Sync>);
    /// What arrived since the last call, oldest first.
    fn take_arrivals(&self) -> Vec<Arrival>;

    /// Register the OS's own file-open delivery against this instance's inbox. On macOS a file
    /// opened from Finder ("Open With", a double-click, `open -a`) is delivered as a
    /// `kAEOpenDocuments` Apple Event, never in `argv` (§8.5); this makes it arrive as an
    /// [`Arrival`] exactly like a single-instance hand-off, so the app's per-frame drain opens it
    /// with no new path. A no-op on Linux and Windows, where the launch path is in `argv`. Call
    /// once, on the main thread, after the window and NSApplication exist.
    fn install_open_event_source(&self) {}
}

/// Arrivals waiting for the app, and how to wake it. Shared by the real adapter and the fake.
#[derive(Default)]
struct Inbox {
    arrivals: Mutex<Vec<Arrival>>,
    wake: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl Inbox {
    fn push(&self, arrival: Arrival) {
        self.arrivals.lock().expect("inbox").push(arrival);
        if let Some(wake) = &*self.wake.lock().expect("inbox wake") {
            wake();
        }
    }

    fn set_wake(&self, wake: Box<dyn Fn() + Send + Sync>) {
        *self.wake.lock().expect("inbox wake") = Some(wake);
    }

    fn take(&self) -> Vec<Arrival> {
        std::mem::take(&mut *self.arrivals.lock().expect("inbox"))
    }
}

/// The [`Arrival`] a macOS `kAEOpenDocuments` Apple Event's file list becomes: the first file,
/// opened as the new Image. zimged shows one image at a time, so any further files are dropped,
/// matching a multi-file drop (§4.6). An empty list yields nothing to open.
// Only [`apple_events`] calls this, but the test covers it on every platform.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn open_arrival(paths: Vec<PathBuf>) -> Option<Arrival> {
    paths.into_iter().next().map(Arrival::Open)
}

/// macOS delivers a file opened from Finder through an Apple Event, never in `argv`; this module
/// registers the handler that turns it into an [`Arrival`]. Compiled only on macOS.
#[cfg(target_os = "macos")]
mod apple_events;

// ---- The real adapter ----

/// How long a starting process waits for the running one to answer. A responsive instance answers
/// in well under a millisecond; this only bounds the wait on a hung one.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(1);
/// Start of every message, so a stray connection can't make the window open something.
const MAGIC: &[u8; 8] = b"zimged1\n";
/// The listener's answer once the arrival is queued.
const ACK: &[u8; 2] = b"ok";
/// Longest path accepted, in encoded bytes.
const MAX_PATH_BYTES: u32 = 64 * 1024;

/// The real single-instance socket.
pub struct LocalSocketInstance {
    /// Unix: the socket file. Windows: the pipe name.
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(windows)]
    pipe: String,
    inbox: Arc<Inbox>,
}

impl LocalSocketInstance {
    /// The per-user socket: `$XDG_RUNTIME_DIR/zimged.sock` (else `zimged-<user>.sock` in the
    /// per-user temp dir) on Unix, the `zimged-<user>` named pipe on Windows.
    pub fn new() -> Self {
        #[cfg(unix)]
        {
            match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
                Some(dir) => Self::in_dir(Path::new(&dir)),
                None => Self::at(std::env::temp_dir().join(format!("zimged-{}.sock", user()))),
            }
        }
        #[cfg(windows)]
        {
            Self {
                pipe: format!("zimged-{}", user()),
                inbox: Arc::default(),
            }
        }
    }

    /// The socket `zimged.sock` in this runtime directory (tests use a temp dir).
    #[cfg(unix)]
    pub fn in_dir(dir: &Path) -> Self {
        Self::at(dir.join("zimged.sock"))
    }

    #[cfg(unix)]
    fn at(socket: PathBuf) -> Self {
        Self {
            socket,
            inbox: Arc::default(),
        }
    }

    fn name(&self) -> io::Result<Name<'_>> {
        #[cfg(unix)]
        {
            use interprocess::local_socket::GenericFilePath;
            self.socket.as_path().to_fs_name::<GenericFilePath>()
        }
        #[cfg(windows)]
        {
            use interprocess::local_socket::GenericNamespaced;
            self.pipe.as_str().to_ns_name::<GenericNamespaced>()
        }
    }

    fn connect(&self) -> io::Result<LocalSocketStream> {
        ConnectOptions::new().name(self.name()?).connect_sync()
    }

    /// Send `path` and wait for the answer.
    fn offer(&self, mut stream: LocalSocketStream, path: Option<&Path>) -> io::Result<()> {
        stream.set_recv_timeout(Some(ANSWER_TIMEOUT))?;
        stream.set_send_timeout(Some(ANSWER_TIMEOUT))?;
        let mut message = MAGIC.to_vec();
        match path {
            None => message.extend_from_slice(&u32::MAX.to_le_bytes()),
            Some(path) => {
                let bytes = encode_path(path);
                message.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                message.extend_from_slice(&bytes);
            }
        }
        stream.write_all(&message)?;
        stream.flush()?;
        let mut answer = [0; ACK.len()];
        stream.read_exact(&mut answer)?;
        if &answer == ACK {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected answer",
            ))
        }
    }

    fn listen(&self) -> io::Result<()> {
        let listener = match self.create_listener() {
            Ok(listener) => listener,
            #[cfg(unix)]
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                // Left behind by an instance that didn't exit cleanly: we just failed to connect
                // to it, so nothing listens there. (If another process started at the same moment
                // and is listening now, this takes the name from it: both then run, no harm.)
                std::fs::remove_file(&self.socket)?;
                self.create_listener()?
            }
            Err(e) => return Err(e),
        };
        let inbox = self.inbox.clone();
        std::thread::Builder::new()
            .name("single-instance".into())
            .spawn(move || {
                for stream in listener.incoming().filter_map(Result::ok) {
                    // One at a time; a client that stalls is cut off by the timeout.
                    if let Some(arrival) = receive(stream) {
                        inbox.push(arrival);
                    }
                }
            })?;
        Ok(())
    }

    fn create_listener(&self) -> io::Result<LocalSocketListener> {
        let options = ListenerOptions::new().name(self.name()?);
        #[cfg(target_os = "linux")]
        let options = {
            use interprocess::os::unix::local_socket::ListenerOptionsExt;
            options.mode(0o600)
        };
        options.create_sync()
    }
}

impl Default for LocalSocketInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleInstance for LocalSocketInstance {
    fn hand_off_or_listen(&self, path: Option<&Path>) -> HandOff {
        // A relative path means nothing in the other process's working directory.
        let path = path.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_owned()));
        if let Ok(stream) = self.connect() {
            if self.offer(stream, path.as_deref()).is_ok() {
                return HandOff::Answered;
            }
            // Connected but no answer: a hung instance holds the socket. Run alone.
            return HandOff::Alone;
        }
        match self.listen() {
            Ok(()) => HandOff::Listening,
            Err(_) => HandOff::Alone,
        }
    }

    fn set_wake(&self, wake: Box<dyn Fn() + Send + Sync>) {
        self.inbox.set_wake(wake);
    }

    fn take_arrivals(&self) -> Vec<Arrival> {
        self.inbox.take()
    }

    #[cfg(target_os = "macos")]
    fn install_open_event_source(&self) {
        apple_events::install(self.inbox.clone());
    }
}

/// Read one message and answer it. `None` for anything malformed (no answer is sent).
fn receive(mut stream: LocalSocketStream) -> Option<Arrival> {
    stream.set_recv_timeout(Some(ANSWER_TIMEOUT)).ok()?;
    stream.set_send_timeout(Some(ANSWER_TIMEOUT)).ok()?;
    let mut magic = [0; MAGIC.len()];
    stream.read_exact(&mut magic).ok()?;
    if &magic != MAGIC {
        return None;
    }
    let mut len = [0; 4];
    stream.read_exact(&mut len).ok()?;
    let arrival = match u32::from_le_bytes(len) {
        u32::MAX => Arrival::Raise,
        len if len <= MAX_PATH_BYTES => {
            let mut bytes = vec![0; len as usize];
            stream.read_exact(&mut bytes).ok()?;
            Arrival::Open(decode_path(&bytes)?)
        }
        _ => return None,
    };
    stream.write_all(ACK).ok()?;
    stream.flush().ok()?;
    Some(arrival)
}

#[cfg(unix)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn decode_path(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(windows)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(windows)]
fn decode_path(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(PathBuf::from(std::ffi::OsString::from_wide(&wide)))
}

/// The user name, to keep users' sockets apart where the location is shared.
fn user() -> String {
    let name = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

// ---- The fake ----

/// In-memory fake. By default no other instance runs, so [`SingleInstance::hand_off_or_listen`]
/// returns [`HandOff::Listening`]; [`MemoryInstance::with_running_instance`] answers instead.
/// [`MemoryInstance::arrive`] plays another process handing this one a path.
#[derive(Default)]
pub struct MemoryInstance {
    running_elsewhere: bool,
    offered: Mutex<Vec<Option<PathBuf>>>,
    inbox: Inbox,
}

impl MemoryInstance {
    /// Another instance is already running and answers.
    pub fn with_running_instance() -> Self {
        Self {
            running_elsewhere: true,
            ..Self::default()
        }
    }

    /// Every [`SingleInstance::hand_off_or_listen`] call's path, in order.
    pub fn offered(&self) -> Vec<Option<PathBuf>> {
        self.offered.lock().expect("offered").clone()
    }

    /// Another process hands this one `path` (`None`: started without a file).
    pub fn arrive(&self, path: Option<PathBuf>) {
        self.inbox.push(match path {
            Some(path) => Arrival::Open(path),
            None => Arrival::Raise,
        });
    }
}

impl SingleInstance for MemoryInstance {
    fn hand_off_or_listen(&self, path: Option<&Path>) -> HandOff {
        let path = path.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_owned()));
        self.offered.lock().expect("offered").push(path);
        if self.running_elsewhere {
            HandOff::Answered
        } else {
            HandOff::Listening
        }
    }

    fn set_wake(&self, wake: Box<dyn Fn() + Send + Sync>) {
        self.inbox.set_wake(wake);
    }

    fn take_arrivals(&self) -> Vec<Arrival> {
        self.inbox.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_arrival_opens_the_first_file_and_ignores_the_rest() {
        assert_eq!(open_arrival(vec![]), None);
        assert_eq!(
            open_arrival(vec![PathBuf::from("/a.png")]),
            Some(Arrival::Open(PathBuf::from("/a.png"))),
        );
        // A multi-file open keeps only the first, like a multi-file drop (§4.6).
        assert_eq!(
            open_arrival(vec![PathBuf::from("/a.png"), PathBuf::from("/b.png")]),
            Some(Arrival::Open(PathBuf::from("/a.png"))),
        );
    }
}

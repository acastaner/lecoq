//! The real single-instance socket (§8.4), in a temp runtime dir: no window needed.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use lecoq_platform::{Arrival, HandOff, LocalSocketInstance, SingleInstance};

fn wait_for_arrivals(instance: &LocalSocketInstance) -> Vec<Arrival> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let arrivals = instance.take_arrivals();
        if !arrivals.is_empty() || Instant::now() > deadline {
            return arrivals;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_first_process_listens_and_a_second_hands_it_the_path() {
    let runtime = tempfile::tempdir().unwrap();
    let running = LocalSocketInstance::in_dir(runtime.path());
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = woken.clone();
    running.set_wake(Box::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(running.hand_off_or_listen(None), HandOff::Listening);

    let starting = LocalSocketInstance::in_dir(runtime.path());
    let image = Path::new("/pictures/holiday 1.jpg");
    assert_eq!(starting.hand_off_or_listen(Some(image)), HandOff::Answered);

    assert_eq!(
        wait_for_arrivals(&running),
        vec![Arrival::Open(image.to_owned())]
    );
    assert!(woken.load(Ordering::SeqCst) >= 1);
    // The process that handed off didn't start listening.
    assert!(starting.take_arrivals().is_empty());
}

#[test]
fn a_relative_path_arrives_absolute() {
    let runtime = tempfile::tempdir().unwrap();
    let running = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(running.hand_off_or_listen(None), HandOff::Listening);

    let starting = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(
        starting.hand_off_or_listen(Some(Path::new("shot.png"))),
        HandOff::Answered
    );

    let expected: PathBuf = std::env::current_dir().unwrap().join("shot.png");
    assert_eq!(wait_for_arrivals(&running), vec![Arrival::Open(expected)]);
}

#[test]
fn a_process_started_without_a_file_only_raises_the_running_window() {
    let runtime = tempfile::tempdir().unwrap();
    let running = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(running.hand_off_or_listen(None), HandOff::Listening);

    let starting = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(starting.hand_off_or_listen(None), HandOff::Answered);

    assert_eq!(wait_for_arrivals(&running), vec![Arrival::Raise]);
}

#[test]
fn a_socket_left_behind_by_a_dead_instance_is_taken_over() {
    let runtime = tempfile::tempdir().unwrap();
    // A crashed instance leaves its socket file with nobody listening on it.
    let dead = std::os::unix::net::UnixListener::bind(runtime.path().join("lecoq.sock")).unwrap();
    drop(dead);
    assert!(runtime.path().join("lecoq.sock").exists());

    let starting = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(
        starting.hand_off_or_listen(Some(Path::new("/a.png"))),
        HandOff::Listening
    );

    let next = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(
        next.hand_off_or_listen(Some(Path::new("/b.png"))),
        HandOff::Answered
    );
    assert_eq!(
        wait_for_arrivals(&starting),
        vec![Arrival::Open(PathBuf::from("/b.png"))]
    );
}

#[test]
fn a_stray_connection_opens_nothing() {
    use std::io::Write;
    let runtime = tempfile::tempdir().unwrap();
    let running = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(running.hand_off_or_listen(None), HandOff::Listening);

    let mut stray =
        std::os::unix::net::UnixStream::connect(runtime.path().join("lecoq.sock")).unwrap();
    stray.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
    drop(stray);

    // A genuine hand-off after it still gets through, and is the only arrival.
    let starting = LocalSocketInstance::in_dir(runtime.path());
    assert_eq!(
        starting.hand_off_or_listen(Some(Path::new("/c.png"))),
        HandOff::Answered
    );
    assert_eq!(
        wait_for_arrivals(&running),
        vec![Arrival::Open(PathBuf::from("/c.png"))]
    );
}

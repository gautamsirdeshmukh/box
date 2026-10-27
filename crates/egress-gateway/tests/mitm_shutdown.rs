#![cfg(feature = "tls-intercept")]

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Barrier, Mutex, mpsc};
use std::time::Duration;

use egress_gateway::{
    CapabilitySet, EffectAttempt, EffectInterceptor, EffectPermit, MitmConfig, MitmInterceptor,
};

struct RefuseEffects;

impl EffectInterceptor for RefuseEffects {
    fn intercept(&self, _: &EffectAttempt<'_>) -> io::Result<Box<dyn EffectPermit>> {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    }
}

#[test]
fn shutdown_does_not_need_the_bound_unix_path() {
    let directory = std::env::temp_dir().join(format!("egress-stop-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("bound");
    let moved = directory.join("moved");
    let handle = MitmInterceptor::start(
        MitmConfig {
            unix_socket_path: Some(path.clone()),
            ..MitmConfig::default()
        },
        CapabilitySet::default(),
        Arc::new(RefuseEffects),
    )
    .unwrap();
    std::fs::rename(path, &moved).unwrap();

    let (sent, received) = mpsc::channel();
    let stopping = std::thread::spawn(move || {
        handle.shutdown();
        drop(handle);
        sent.send(()).unwrap();
    });
    let completed = received.recv_timeout(Duration::from_secs(2)).is_ok();
    let rescue = if completed {
        None
    } else {
        let stream = UnixStream::connect(&moved).unwrap();
        received
            .recv_timeout(Duration::from_secs(2))
            .expect("the rescue connection releases the old accept loop");
        Some(stream)
    };
    stopping.join().unwrap();
    drop(rescue);
    std::fs::remove_file(moved).unwrap();
    std::fs::remove_dir(directory).unwrap();

    assert!(
        completed,
        "shutdown depended on connecting to the bound path"
    );
}

struct HeldEffect {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl EffectInterceptor for HeldEffect {
    fn intercept(&self, _: &EffectAttempt<'_>) -> io::Result<Box<dyn EffectPermit>> {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    }
}

#[test]
fn shutdown_does_not_wait_for_an_accepted_handler() {
    let (entered, accepted) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let handle = MitmInterceptor::start(
        MitmConfig {
            max_connections: 1,
            ..MitmConfig::default()
        },
        CapabilitySet::default(),
        Arc::new(HeldEffect {
            entered,
            release: Mutex::new(held),
        }),
    )
    .unwrap();
    let mut client = TcpStream::connect(("127.0.0.1", handle.port().unwrap())).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .write_all(b"GET http://example.test/ HTTP/1.1\r\nHost: example.test\r\n\r\n")
        .unwrap();
    accepted.recv_timeout(Duration::from_secs(2)).unwrap();

    let (sent, received) = mpsc::channel();
    let stopping = std::thread::spawn(move || {
        drop(handle);
        sent.send(()).unwrap();
    });
    let completed = received.recv_timeout(Duration::from_secs(2)).is_ok();
    release.send(()).unwrap();
    if !completed {
        received.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    stopping.join().unwrap();

    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert!(completed, "shutdown waited for an accepted handler");
    assert!(response.starts_with("HTTP/1.1 403 "));
}

#[test]
fn concurrent_shutdown_calls_are_idempotent() {
    let handle = Arc::new(
        MitmInterceptor::start(
            MitmConfig::default(),
            CapabilitySet::default(),
            Arc::new(RefuseEffects),
        )
        .unwrap(),
    );
    let ready = Arc::new(Barrier::new(8));
    let mut callers = Vec::new();
    for _ in 0..8 {
        let handle = handle.clone();
        let ready = ready.clone();
        callers.push(std::thread::spawn(move || {
            ready.wait();
            handle.shutdown();
        }));
    }
    for caller in callers {
        caller.join().unwrap();
    }
    let (sent, received) = mpsc::channel();
    let stopping = std::thread::spawn(move || {
        drop(handle);
        sent.send(()).unwrap();
    });
    received
        .recv_timeout(Duration::from_secs(2))
        .expect("the last handle completes after concurrent shutdown requests");
    stopping.join().unwrap();
}

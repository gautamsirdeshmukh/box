#![cfg(feature = "tls-intercept")]

use std::io;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, mpsc};
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

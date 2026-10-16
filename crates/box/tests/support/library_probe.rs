//! An external caller of the Box library.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

fn main() -> Result<ExitCode, std::boxed::Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().expect("mode");
    let config = PathBuf::from(args.next().expect("config"));
    if mode == "refuse" {
        let error = match strands_box::start(config) {
            Ok(_) => panic!("a missing configuration must fail in start"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("box.toml"), "{error}");
        return Ok(ExitCode::SUCCESS);
    }

    let directory = config.parent().expect("config directory");
    let workspace = directory.join("workspace");
    let live = directory.join("box/private/live.json");
    let box_process = strands_box::start(config.clone())?;
    until(|| workspace.join("ready").exists());
    assert!(live.exists(), "the running Box publishes its live record");
    assert!(
        strands_box::start(config).is_err(),
        "a second Box is refused"
    );
    let runtime = tokio::runtime::Runtime::new()?;

    if mode == "drop" {
        let started = Instant::now();
        runtime.block_on(async { drop(box_process) });
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!live.exists(), "drop completes Box cleanup");
        return Ok(ExitCode::SUCCESS);
    }

    std::fs::write(workspace.join("release"), b"exit")?;
    until(|| !live.exists());
    runtime
        .block_on(async { box_process.wait() })
        .map_err(Into::into)
}

fn until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "Box did not complete in time");
        std::thread::sleep(Duration::from_millis(10));
    }
}

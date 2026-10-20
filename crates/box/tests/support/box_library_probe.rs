//! Test-support process for the public Box library entry point.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    match arguments.as_slice() {
        [mode, config] if mode == "run" => strands_box::run(PathBuf::from(config), Vec::new()),
        [mode, config, marker, release] if mode == "run-twice" => run_twice(
            PathBuf::from(config),
            PathBuf::from(marker),
            PathBuf::from(release),
        ),
        [mode, marker, release] if mode == "workload" => {
            workload(Path::new(marker), Path::new(release))
        }
        _ => {
            eprintln!(
                "usage: box-library-probe <run CONFIG | run-twice CONFIG MARKER RELEASE | \
                 workload MARKER RELEASE>"
            );
            ExitCode::from(2)
        }
    }
}

fn run_twice(config: PathBuf, marker: PathBuf, release: PathBuf) -> ExitCode {
    let workload = vec![
        OsString::from("workload"),
        marker.clone().into_os_string(),
        release.clone().into_os_string(),
    ];
    let first = std::thread::spawn({
        let config = config.clone();
        move || strands_box::run(config, workload)
    });

    let deadline = Instant::now() + TIMEOUT;
    while !marker.exists() {
        if first.is_finished() {
            let exit = first.join().expect("the first run thread");
            eprintln!(
                "box-library-probe: the first run exited before its workload started: {exit:?}"
            );
            return ExitCode::FAILURE;
        }
        if Instant::now() >= deadline {
            std::fs::write(&release, b"release").expect("release the timed-out workload");
            let _ = first.join();
            eprintln!("box-library-probe: the first workload did not start before the timeout");
            return ExitCode::FAILURE;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let second = strands_box::run(config, Vec::new());
    std::fs::write(&release, b"release").expect("release the first workload");
    let first = first.join().expect("the first run thread");

    if first != ExitCode::SUCCESS {
        eprintln!("box-library-probe: the first run failed: {first:?}");
        return ExitCode::FAILURE;
    }
    if second != ExitCode::FAILURE {
        eprintln!("box-library-probe: the second run was not refused: {second:?}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn workload(marker: &Path, release: &Path) -> ExitCode {
    std::fs::write(marker, b"started").expect("write the started marker");
    let deadline = Instant::now() + TIMEOUT;
    while !release.exists() {
        if Instant::now() >= deadline {
            eprintln!("box-library-probe: the workload was not released before the timeout");
            return ExitCode::FAILURE;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    ExitCode::SUCCESS
}

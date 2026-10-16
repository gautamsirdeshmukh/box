//! The public library starts, supervises, and cleans up one workload.

use std::io::{Read, Seek};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[path = "support/fixture.rs"]
mod fixture;

#[test]
fn start_returns_before_exit_and_cleanup_does_not_wait_for_the_caller() {
    if !fixture::namespace_launcher_is_usable() {
        eprintln!("skipping: this host cannot launch a contained workload");
        return;
    }
    let home = configured();
    let output = probe(home.path(), "wait");
    assert_eq!(output.status.code(), Some(7), "{output:?}");
}

#[test]
fn dropping_the_box_stops_the_workload_and_cleans_up() {
    if !fixture::namespace_launcher_is_usable() {
        eprintln!("skipping: this host cannot launch a contained workload");
        return;
    }
    let home = configured();
    let output = probe(home.path(), "drop");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn start_returns_configuration_errors() {
    let home = fixture::short_temporary_home();
    let output = probe(home.path(), "refuse");
    assert!(output.status.success(), "{output:?}");
}

fn configured() -> tempfile::TempDir {
    let home = fixture::short_temporary_home();
    let workspace = home.path().join("workspace");
    let directory = home.path().join("box");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let script = r#"printf ready > "$1/ready"
while [ ! -f "$1/release" ]; do
    if [ "$SECONDS" -ge 20 ]; then exit 99; fi
done
exit 7"#;
    let command = toml::Value::Array(
        ["/bin/bash", "-c", script, "--", workspace.to_str().unwrap()]
            .into_iter()
            .map(|value| toml::Value::String(value.to_string()))
            .collect(),
    );
    std::fs::write(
        home.path().join("box.toml"),
        format!(
            r#"name = "library"
box_dir = {directory:?}
[agent]
command = {command}
workspace = {workspace:?}
[agent.filesystem]
read = [{workspace:?}]
write = [{workspace:?}]
"#
        ),
    )
    .unwrap();
    home
}

fn probe(home: &Path, mode: &str) -> Output {
    let stdout = tempfile::tempfile().unwrap();
    let stderr = tempfile::tempfile().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_box-library-probe"))
        .arg(mode)
        .arg(home.join("box.toml"))
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = Output {
        status,
        stdout: captured(stdout),
        stderr: captured(stderr),
    };
    assert!(!timed_out, "library probe did not finish: {output:?}");
    output
}

fn captured(mut file: std::fs::File) -> Vec<u8> {
    file.rewind().unwrap();
    let mut bytes = Vec::new();
    file.take(1024 * 1024).read_to_end(&mut bytes).unwrap();
    bytes
}

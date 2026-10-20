//! Public library entry-point behavior through a test-support host process.

mod support {
    pub mod bounded_output;
    pub mod fixture;
}

use std::path::Path;
use std::process::Command;

use support::bounded_output::bounded_output;
use support::fixture::{namespace_launcher_is_usable, short_temporary_home};

const PROBE: &str = env!("CARGO_BIN_EXE_box-library-probe");
const REPEAT_REFUSAL: &str =
    "strands-box: refusing to run: this process already attempted to run a box";

#[test]
fn an_invalid_configuration_is_refused_with_the_parse_error() {
    let directory = tempfile::tempdir().expect("create the test directory");
    let config = directory.path().join("box.toml");
    std::fs::write(&config, "name = [").expect("write the invalid configuration");
    let config = config
        .canonicalize()
        .expect("resolve the configuration path");

    let mut command = Command::new(PROBE);
    command.arg("run").arg(&config);
    let output = bounded_output(command);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "strands-box: error: cannot parse config file {}",
            config.display()
        )),
        "{stderr}"
    );
}

#[test]
fn a_second_library_run_is_refused_while_the_first_is_running() {
    if !namespace_launcher_is_usable() {
        println!("skipping: this host cannot launch a contained Box workload");
        return;
    }

    let home = short_temporary_home();
    let workspace = home.path().join("workspace");
    let control = workspace.join("control");
    let box_directory = home.path().join("box");
    std::fs::create_dir_all(&control).expect("create the control directory");
    std::fs::create_dir(&box_directory).expect("create the box directory");
    make_private(&box_directory);

    let config = workspace.join("box.toml");
    let marker = control.join("started");
    let release = control.join("release");
    write_config(&config, &box_directory, &workspace, &control);

    let mut command = Command::new(PROBE);
    command
        .arg("run-twice")
        .arg(&config)
        .arg(&marker)
        .arg(&release)
        .env("HOME", home.path());
    let output = bounded_output(command);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.lines().any(|line| line == REPEAT_REFUSAL),
        "{stderr}"
    );
}

fn write_config(config: &Path, box_directory: &Path, workspace: &Path, control: &Path) {
    let quoted = |path: &Path| toml::Value::String(path.display().to_string()).to_string();
    std::fs::write(
        config,
        format!(
            "name = \"library-probe\"\nbox_dir = {}\n\n\
             [agent]\ncommand = [{}]\nworkspace = {}\n\n\
             [agent.filesystem]\nwrite = [{}]\nmetadata = [{}]\n",
            quoted(box_directory),
            quoted(Path::new(PROBE)),
            quoted(workspace),
            quoted(control),
            quoted(control),
        ),
    )
    .expect("write the configuration");
}

#[cfg(unix)]
fn make_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .expect("make the box directory private");
}

#[cfg(not(unix))]
fn make_private(_path: &Path) {}

//! `strands-box` — run workloads inside a zero-trust box.
//!
//! **The command line and library share one implementation.** `box.toml` states a box's authority
//! and both act on it. There was a `[lib]` façade beside this — `Box`, `Run`, `Running`, `BoxConfig` — which
//! was a second vocabulary for the same thing: a caller staged program *content* through
//! `Run::staged` where an operator names `--code <dir> -- python main.py`. Two spellings of one idea,
//! and only one of them was reachable by anyone reading `--help`.
//!
//! | Module | Owns |
//! |---|---|
//! | `command` | one module per verb, and the argv parser |
//! | `record` | what a box *is*, before anything is running |
//! | `run` | one box's trusted half, alive for one run |
//! | `error` | the typed taxonomy, one variant group per phase |

#![warn(unreachable_pub)]

mod command;
mod error;
mod record;
mod run;
#[cfg(test)]
mod test_support;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use tokio::runtime::Runtime;
use tokio::sync::oneshot;

pub use error::BoxError;

/// A running Box that stops and cleans up when dropped.
pub struct Box {
    task: Option<JoinHandle<Result<ExitCode, BoxError>>>,
    stop: Option<oneshot::Sender<()>>,
}

/// Start one Box in this host process before application threads or secrets exist.
pub fn start(config: PathBuf) -> Result<Box, BoxError> {
    Box::start(config, Vec::new())
}

impl Box {
    pub(crate) fn start(config: PathBuf, args: Vec<std::ffi::OsString>) -> Result<Self, BoxError> {
        static STARTED: AtomicBool = AtomicBool::new(false);
        if STARTED.swap(true, Ordering::SeqCst) {
            return Err(error::Internal::AlreadyStarted.into());
        }
        run::hardening::Hardening::apply().map_err(error::Internal::Hardening)?;
        let (stop, stopped) = oneshot::channel();
        let (started, ready) = std::sync::mpsc::sync_channel(1);
        let task = std::thread::Builder::new()
            .name("box-runtime".into())
            .spawn(move || {
                let runtime = Runtime::new().map_err(error::Internal::Runtime)?;
                let workload = runtime.block_on(command::run::start(&config, &args, stopped))?;
                let _ = started.send(());
                runtime.block_on(workload)
            })
            .map_err(error::Internal::Thread)?;
        let box_process = Self {
            task: Some(task),
            stop: Some(stop),
        };
        if ready.recv().is_err() {
            return match box_process.wait() {
                Err(error) => Err(error),
                Ok(_) => Err(error::Internal::Task.into()),
            };
        }
        Ok(box_process)
    }

    /// Wait for this Box to finish and return its exit result.
    pub fn wait(mut self) -> Result<ExitCode, BoxError> {
        let task = self.task.take().expect("wait owns the Box task");
        task.join().map_err(|_| error::Internal::Task)?
    }
}

impl Drop for Box {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            let _ = task.join();
        }
    }
}

/// The standalone Box command-line entry point.
pub fn cli_main() -> std::process::ExitCode {
    command::cli_main()
}

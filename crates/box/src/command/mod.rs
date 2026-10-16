//! One module per CLI verb: what each of `strands-box`'s commands does.
//!
//! | Lifetime | Verbs |
//! |---|---|
//! | workspace | [`policy`] |
//! | run | [`run`] |
//!
//! `run::configure` is not a verb. It is the creation mechanism `run` uses, and `run` is the only
//! caller.

pub(crate) mod cli;
pub(crate) mod policy;
pub(crate) mod run;

use std::process::ExitCode;

use clap::Parser as _;

use crate::command::cli::{Cli, Command, PolicyCommand};
use crate::error::{BoxError, Internal};

pub(crate) fn cli_main() -> ExitCode {
    let outcome = match Cli::parse().command {
        Command::Run { config, workload } => {
            crate::Box::start(config, workload).and_then(crate::Box::wait)
        }
        Command::Policy {
            command: PolicyCommand::GenerateSchema { config, output_dir },
        } => generate_schema(&config, &output_dir),
    };
    match outcome {
        Ok(code) => code,
        Err(error) => {
            match &error.0 {
                Internal::Hardening(_) | Internal::Runtime(_) => eprintln!("strands-box: {error}"),
                _ => eprintln!("strands-box: error: {error}"),
            }
            ExitCode::FAILURE
        }
    }
}

fn generate_schema(
    config: &std::path::Path,
    output_dir: &std::path::Path,
) -> Result<ExitCode, BoxError> {
    let runtime = tokio::runtime::Runtime::new().map_err(Internal::Runtime)?;
    runtime.block_on(policy::generate_schema(config, output_dir))
}

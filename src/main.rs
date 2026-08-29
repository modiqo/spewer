//! Spewer command-line entrypoint.

use spewer::error::ErrorKind;
use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match spewer::cli::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spewer: {error}");
            if error.kind() == ErrorKind::InvalidInput {
                eprintln!("Run 'spewer help' for the lifecycle and command guide.");
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

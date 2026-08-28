//! Spewer command-line entrypoint.

use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match spewer::cli::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spewer: {error}");
            ExitCode::FAILURE
        }
    }
}

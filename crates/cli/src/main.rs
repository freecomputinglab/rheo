use std::error::Error;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = rheo::run() {
        eprintln!("Error: {e}");
        let mut source = e.source();
        while let Some(s) = source {
            eprintln!("Caused by: {s}");
            source = s.source();
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

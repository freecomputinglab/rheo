fn main() {
    // Re-export the rheo-cli entry point
    if let Err(e) = rheo_cli::run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

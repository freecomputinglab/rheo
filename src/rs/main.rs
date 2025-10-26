use rheo::{Cli, Result};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing with the appropriate verbosity level
    rheo::logging::init(cli.verbosity())?;

    cli.run()
}

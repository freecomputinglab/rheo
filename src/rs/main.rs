use rheo::{Cli, Result};

fn main() -> Result<()> {
    let cli = Cli::parse();

    rheo::logging::init(cli.verbosity())?;

    cli.run()
}

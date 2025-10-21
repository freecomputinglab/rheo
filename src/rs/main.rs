use anyhow::Result;
use rheo::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run()
}

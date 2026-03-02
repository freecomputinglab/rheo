use rheo_core::Result;

fn main() -> Result<()> {
    let cli = rheo_cli::Cli::parse();

    rheo_cli::init_logging(cli.verbose, cli.quiet)?;

    cli.run()
}

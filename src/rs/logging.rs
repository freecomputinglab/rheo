use anyhow::Result;
use tracing::Level;
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Verbosity level for CLI output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

impl Verbosity {
    /// Convert verbosity level to tracing Level filter
    fn to_level_filter(self) -> Level {
        match self {
            Verbosity::Quiet => Level::ERROR,
            Verbosity::Normal => Level::INFO,
            Verbosity::Verbose => Level::DEBUG,
        }
    }
}

/// Initialize the tracing subscriber with appropriate configuration
///
/// This sets up colored, human-friendly output for TTY and plain output for pipes/files.
/// Respects RUST_LOG environment variable and CLI verbosity flags.
pub fn init(verbosity: Verbosity) -> Result<()> {
    // Check if stdout is a TTY for colored output
    let is_tty = atty::is(atty::Stream::Stdout);

    // Build the environment filter
    // RUST_LOG takes precedence if set, otherwise use verbosity level
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default filter: show our crate at the specified level, hide other crates
        EnvFilter::new(format!(
            "rheo={}",
            verbosity.to_level_filter().as_str().to_lowercase()
        ))
    });

    // Build the formatter with appropriate styling
    let fmt_layer = fmt::layer()
        .with_target(false) // Don't show target (module path) in normal output
        .with_level(true) // Show log level
        .with_ansi(is_tty) // Only use colors if outputting to a TTY
        .compact(); // Use compact format similar to cargo

    // Initialize the subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;

    Ok(())
}

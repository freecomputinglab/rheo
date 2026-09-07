use crate::{Result, RheoError};
use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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
    // Check if stderr is a TTY for colored output (logs are written there)
    let is_tty = atty::is(atty::Stream::Stderr);

    // Build the environment filter
    // RUST_LOG takes precedence if set, otherwise use verbosity level
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // EnvFilter matches targets by string prefix, not crate boundary, so a
        // bare "rheo" directive already happens to catch every "rheo_*" crate.
        // List rheo_html explicitly so the dev server's lifecycle logs (bound
        // URL, rebuild, reload) stay visible on purpose rather than by luck of
        // naming, and survive the directive ever being tightened.
        let level = verbosity.to_level_filter().as_str().to_lowercase();
        EnvFilter::new(format!("rheo={level},rheo_html={level}"))
    });

    // Build the formatter with appropriate styling.
    //
    // TIMESTAMPS ARE ON, and a `watch` session is why: its log is a timeline —
    // this save, that rebuild, the next — and without them the only way to
    // learn what a rebuild cost was to pipe stderr through `date` a line at a
    // time. `uptime()` rather than a wall clock: what a reader of these lines
    // wants is the interval between two of them, and seconds-since-start
    // subtracts by eye where `14:32:07.918` does not.
    let fmt_layer = fmt::layer()
        .with_target(false) // Don't show target (module path) in normal output
        .with_level(true) // Show log level
        .with_ansi(is_tty) // Only use colors if outputting to a TTY
        .with_timer(fmt::time::uptime())
        .compact() // Use compact format similar to cargo
        .with_writer(std::io::stderr); // Diagnostics are not compiled output

    // Initialize the subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init()
        .map_err(|e| RheoError::LoggingInit {
            message: format!("{}", e),
        })?;

    Ok(())
}

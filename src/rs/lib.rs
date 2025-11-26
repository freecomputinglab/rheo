pub mod assets;
pub mod cli;
pub mod compile;
pub mod config;
pub mod epub;
pub mod error;
pub mod logging;
pub mod output;
pub mod project;
pub mod server;
pub mod watch;
pub mod world;

pub use cli::Cli;
pub use config::RheoConfig;
pub use error::RheoError;

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

pub mod cli;
pub mod compile;
pub mod config;
pub mod project;
pub mod output;
pub mod assets;
pub mod epub;
pub mod world;
pub mod logging;
pub mod error;

pub use cli::Cli;
pub use config::RheoConfig;
pub use error::RheoError;

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

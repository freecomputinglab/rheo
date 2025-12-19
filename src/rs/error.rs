use std::path::PathBuf;
use thiserror::Error;

/// Unified error type for all rheo operations
#[derive(Error, Debug)]
pub enum RheoError {
    /// IO error with additional context
    #[error("IO error while {context}: {source}")]
    Io {
        #[source]
        source: std::io::Error,
        context: String,
    },

    /// Path resolution or validation error
    #[error("Path resolution failed for {path:?}: {reason}")]
    PathResolution { path: PathBuf, reason: String },

    /// Typst compilation error
    #[error("Compilation failed with {count} error(s):\n{errors}")]
    Compilation { count: usize, errors: String },

    /// PDF generation error
    #[error("PDF generation failed with {count} error(s):\n{errors}")]
    PdfGeneration { count: usize, errors: String },

    /// HTML generation error
    #[error("HTML generation failed with {count} error(s):\n{errors}")]
    HtmlGeneration { count: usize, errors: String },

    /// EPUB export error
    #[error("EPUB generation failed with {count} error(s):\n{errors}")]
    EpubGeneration { count: usize, errors: String },

    /// Project configuration detection error
    #[error("Project configuration error: {message}")]
    ProjectConfig { message: String },

    /// Logging initialization error
    #[error("Failed to initialize logging: {message}")]
    LoggingInit { message: String },

    /// Asset copying error
    #[error("Failed to copy asset from {source:?} to {dest:?}: {error}")]
    AssetCopy {
        source: PathBuf,
        dest: PathBuf,
        #[source]
        error: std::io::Error,
    },

    /// File watcher error
    #[error("File watcher error while {context}: {source}")]
    FileWatcher {
        #[source]
        source: notify::Error,
        context: String,
    },

    /// Parse error for invalid input data
    #[error("Parse error: {message}")]
    ParseError { message: String },

    /// Invalid data error
    #[error("Invalid data: {message}")]
    InvalidData { message: String },
}

impl RheoError {
    /// Helper to create an IO error with context
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        RheoError::Io {
            source,
            context: context.into(),
        }
    }

    /// Helper to create a path resolution error
    pub fn path(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        RheoError::PathResolution {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Helper to create a project config error
    pub fn project_config(message: impl Into<String>) -> Self {
        RheoError::ProjectConfig {
            message: message.into(),
        }
    }

    /// Helper to create a file watcher error with context
    pub fn file_watcher(source: notify::Error, context: impl Into<String>) -> Self {
        RheoError::FileWatcher {
            source,
            context: context.into(),
        }
    }

    /// Helper to create a parse error
    pub fn parse_error(message: impl Into<String>) -> Self {
        RheoError::ParseError {
            message: message.into(),
        }
    }

    /// Helper to create an invalid data error
    pub fn invalid_data(message: impl Into<String>) -> Self {
        RheoError::InvalidData {
            message: message.into(),
        }
    }
}

/// Automatic conversion from std::io::Error for convenience
impl From<std::io::Error> for RheoError {
    fn from(error: std::io::Error) -> Self {
        RheoError::Io {
            source: error,
            context: "unknown operation".to_string(),
        }
    }
}

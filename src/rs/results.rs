use crate::OutputFormat;
use std::collections::HashMap;
use tracing::info;

/// Result counts for a single output format
#[derive(Debug, Default, Clone, Copy)]
pub struct FormatResult {
    pub succeeded: usize,
    pub failed: usize,
}

/// Aggregated compilation results across all output formats
#[derive(Debug, Default)]
pub struct CompilationResults {
    results: HashMap<OutputFormat, FormatResult>,
}

impl CompilationResults {
    /// Create a new empty results tracker
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
        }
    }

    /// Record a successful compilation for the given format
    pub fn record_success(&mut self, format: OutputFormat) {
        self.results.entry(format).or_default().succeeded += 1;
    }

    /// Record a failed compilation for the given format
    pub fn record_failure(&mut self, format: OutputFormat) {
        self.results.entry(format).or_default().failed += 1;
    }

    /// Get the result counts for a specific format
    pub fn get(&self, format: OutputFormat) -> FormatResult {
        self.results.get(&format).copied().unwrap_or_default()
    }

    /// Check if any compilations failed
    pub fn has_failures(&self) -> bool {
        self.results.values().any(|r| r.failed > 0)
    }

    /// Log a summary of compilation results for requested formats
    pub fn log_summary(&self, requested_formats: &[OutputFormat]) {
        for format in requested_formats {
            let result = self.get(*format);
            let total = result.succeeded + result.failed;
            if total > 0 {
                if result.failed == 0 {
                    info!(
                        format = format!("{:?}", format),
                        "successfully compiled {} file(s)",
                        result.succeeded
                    );
                } else {
                    info!(
                        format = format!("{:?}", format),
                        "compiled {} file(s), {} failed",
                        result.succeeded,
                        result.failed
                    );
                }
            }
        }
    }
}

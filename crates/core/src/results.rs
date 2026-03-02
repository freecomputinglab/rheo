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
    results: HashMap<String, FormatResult>,
}

impl CompilationResults {
    /// Create a new empty results tracker
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
        }
    }

    /// Record a successful compilation for the given plugin
    pub fn record_success(&mut self, name: &str) {
        self.results.entry(name.to_string()).or_default().succeeded += 1;
    }

    /// Record a failed compilation for the given plugin
    pub fn record_failure(&mut self, name: &str) {
        self.results.entry(name.to_string()).or_default().failed += 1;
    }

    /// Get the result counts for a specific plugin
    pub fn get(&self, name: &str) -> FormatResult {
        self.results.get(name).copied().unwrap_or_default()
    }

    /// Check if any compilations failed
    pub fn has_failures(&self) -> bool {
        self.results.values().any(|r| r.failed > 0)
    }

    /// Log a summary of compilation results for requested plugins
    pub fn log_summary(&self, names: &[&str]) {
        for name in names {
            let result = self.get(name);
            let total = result.succeeded + result.failed;
            if total > 0 {
                if result.failed == 0 {
                    info!(
                        format = *name,
                        "successfully compiled {} file(s)", result.succeeded
                    );
                } else {
                    info!(
                        format = *name,
                        "compiled {} file(s), {} failed", result.succeeded, result.failed
                    );
                }
            }
        }
    }
}

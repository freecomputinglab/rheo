//! Where a build's wall-clock went, and how much output it produced.
//!
//! A build has clean phase seams already — package resolution, the spine scan,
//! the Typst compile, the export, the plugin's writes — and each of them logged
//! that it had happened without saying how long it took. So "the build got
//! slow" could only be answered by deleting features from the project until it
//! got fast again. MEASURED once, on a project that took 29s to build: the
//! whole answer was 360 pages totalling 43 MB, a figure nothing in the build
//! printed and which took a day of bisection to arrive at.
//!
//! Two levels, on purpose. Per-phase lines are `debug!` and carry one `phase`
//! field and one `ms` field so they can be grepped and summed; the summary is a
//! single `info!` line every build prints, because the numbers that identify a
//! pathological project — page count and total bytes — are worth seeing without
//! anyone having thought to ask.

use std::fmt::Write as _;
use std::time::Duration;

/// The phase names, spelled once. A phase runs once per format plugin, so a
/// build's timings sum several samples under most of these.
pub mod phase {
    /// Scanning `.typ` files for package imports, pre-warming them, resolving
    /// the index, and checking declared version floors.
    pub const PACKAGES: &str = "packages";
    /// Typst's embedded, system and configured font scan. Once per build,
    /// shared across every plugin and pass — see `Build::fonts`.
    pub const FONTS: &str = "fonts";
    /// Walking the content directory and applying the spine config.
    pub const SPINE_SCAN: &str = "spine-scan";
    /// Gathering package and project marrow contributions.
    pub const MARROW: &str = "marrow";
    /// Synthesizing the bundle main and the per-vertebra source overlay.
    pub const MOULD: &str = "mould";
    /// `typst::compile` over the whole spine. Usually the dominant phase, and
    /// the one a project's own Typst controls.
    pub const TYPST_COMPILE: &str = "typst-compile";
    /// Turning the compiled bundle into a path→bytes map.
    pub const EXPORT: &str = "export";
    /// Splitting that map into documents and assets and attaching metadata.
    pub const FLATTEN: &str = "flatten";
    /// The format plugin writing its output — for HTML, one file per page.
    pub const PLUGIN_WRITE: &str = "plugin-write";
    /// Applying the project's, packages' and plugin's `copy` globs.
    pub const COPY_GLOBS: &str = "copy-globs";
}

/// One build's phase timings and output totals.
#[derive(Debug, Default)]
pub struct BuildTiming {
    /// INSERTION-ORDERED, not a map, so the summary reads in pipeline order
    /// rather than in whatever order a hash gives; durations for a repeated
    /// phase are summed in place.
    phases: Vec<(&'static str, Duration)>,
    outputs: usize,
    assets: usize,
    bytes: usize,
}

impl BuildTiming {
    /// Add `elapsed` to `phase`, keeping first-seen order.
    pub fn record(&mut self, phase: &'static str, elapsed: Duration) {
        match self.phases.iter_mut().find(|(name, _)| *name == phase) {
            Some((_, total)) => *total += elapsed,
            None => self.phases.push((phase, elapsed)),
        }
    }

    /// Add one plugin's output tally: how many documents, how many raw assets,
    /// and how many bytes across both.
    pub fn record_outputs(&mut self, outputs: usize, assets: usize, bytes: usize) {
        self.outputs += outputs;
        self.assets += assets;
        self.bytes += bytes;
    }

    /// Documents written across every plugin.
    pub fn outputs(&self) -> usize {
        self.outputs
    }

    /// Bytes of COMPILED OUTPUT across every plugin — the bundle's documents
    /// and its `asset()` files. A stylesheet or script the plugin copies in
    /// alongside them is not counted, so this is smaller than `du` over the
    /// output directory and is the figure that scales with the project's own
    /// content.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Sum of every phase — close to but not identical with the build's wall
    /// clock, which also includes whatever sits between the timed seams.
    pub fn total(&self) -> Duration {
        self.phases.iter().map(|(_, d)| *d).sum()
    }

    /// The `n` most expensive phases, longest first. Ties keep their pipeline
    /// order, `sort_by_key` being stable and `phases` being insertion-ordered.
    /// Only the few that dominate belong on the summary line; the rest are in
    /// the `debug!` stream beside it.
    pub fn slowest(&self, n: usize) -> Vec<(&'static str, Duration)> {
        let mut sorted = self.phases.clone();
        sorted.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
        sorted.truncate(n);
        sorted
    }

    /// The one-line summary, e.g. `364 page(s), 3.3 MB in 2.8s (+1 asset(s))
    /// — typst-compile 2.4s, plugin-write 322ms, fonts 28ms`.
    ///
    /// PAGE COUNT AND TOTAL BYTES COME FIRST because they are what identifies a
    /// project emitting per-page data it should emit once — the failure mode
    /// this line exists to make obvious — and a phase breakdown alone does not
    /// show it: the work is spread evenly across the pages that carry it.
    pub fn summary(&self) -> String {
        let mut out = format!(
            "{} page(s), {} in {}",
            self.outputs,
            human_bytes(self.bytes),
            human_duration(self.total()),
        );
        if self.assets > 0 {
            let _ = write!(out, " (+{} asset(s))", self.assets);
        }
        let slowest = self.slowest(3);
        if !slowest.is_empty() {
            let parts: Vec<String> = slowest
                .iter()
                .map(|(name, d)| format!("{name} {}", human_duration(*d)))
                .collect();
            let _ = write!(out, " — {}", parts.join(", "));
        }
        out
    }
}

/// `43.1 MB`, `112.3 kB`, `847 B` — SI units, because the numbers this appears
/// beside are compared against file sizes a reader gets from `du`.
fn human_bytes(bytes: usize) -> String {
    const MB: f64 = 1_000_000.0;
    const KB: f64 = 1_000.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} kB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// `29.4s` above a second, `847ms` below it. Sub-second phases are the common
/// case and reading `0.0s` for each of them says nothing.
fn human_duration(d: Duration) -> String {
    if d.as_secs() >= 1 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_sums_a_repeated_phase_and_keeps_first_seen_order() {
        let mut t = BuildTiming::default();
        t.record(phase::SPINE_SCAN, Duration::from_millis(10));
        t.record(phase::TYPST_COMPILE, Duration::from_millis(100));
        // The same phase again, as a second format plugin would produce.
        t.record(phase::SPINE_SCAN, Duration::from_millis(5));

        assert_eq!(
            t.phases,
            vec![
                (phase::SPINE_SCAN, Duration::from_millis(15)),
                (phase::TYPST_COMPILE, Duration::from_millis(100)),
            ]
        );
        assert_eq!(t.total(), Duration::from_millis(115));
    }

    #[test]
    fn slowest_ranks_by_duration_not_insertion() {
        let mut t = BuildTiming::default();
        t.record(phase::PACKAGES, Duration::from_millis(1));
        t.record(phase::TYPST_COMPILE, Duration::from_millis(900));
        t.record(phase::PLUGIN_WRITE, Duration::from_millis(50));
        t.record(phase::EXPORT, Duration::from_millis(10));

        let names: Vec<&str> = t.slowest(3).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            vec![phase::TYPST_COMPILE, phase::PLUGIN_WRITE, phase::EXPORT]
        );
    }

    /// The figures that identify a pathological project have to be IN the line,
    /// because the whole point is that nobody has to know to ask for them.
    #[test]
    fn summary_names_pages_bytes_total_and_the_worst_phases() {
        let mut t = BuildTiming::default();
        t.record(phase::TYPST_COMPILE, Duration::from_millis(26_900));
        t.record(phase::PLUGIN_WRITE, Duration::from_millis(1_900));
        t.record(phase::EXPORT, Duration::from_millis(400));
        t.record_outputs(360, 0, 43_100_000);

        let s = t.summary();
        assert!(s.contains("360 page(s)"), "{s}");
        assert!(s.contains("43.1 MB"), "{s}");
        assert!(s.contains("29.2s"), "{s}");
        assert!(s.contains("typst-compile 26.9s"), "{s}");
        assert!(!s.contains("asset"), "no assets, so no asset clause: {s}");
    }

    #[test]
    fn summary_mentions_assets_only_when_there_are_some() {
        let mut t = BuildTiming::default();
        t.record(phase::EXPORT, Duration::from_millis(5));
        t.record_outputs(2, 3, 1_500);
        assert!(t.summary().contains("+3 asset(s)"), "{}", t.summary());
    }

    #[test]
    fn units_switch_at_a_second_and_at_a_kilobyte() {
        assert_eq!(human_duration(Duration::from_millis(999)), "999ms");
        assert_eq!(human_duration(Duration::from_millis(1_000)), "1.0s");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_000), "1.0 kB");
        assert_eq!(human_bytes(43_100_000), "43.1 MB");
    }
}

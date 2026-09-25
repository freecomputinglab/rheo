#!/usr/bin/env bash
#
# cold-bench.sh — measures rheo's cold `rheo compile` latency on a real
# project, with repeats and an optional output-identity check.
#
# What it measures: N fresh `rheo compile` runs of PROJECT_DIR timed with
# hyperfine, reporting a mean and a spread rather than a single number — the
# machine this runs on is shared, and single cold runs have swung from 35s to
# 46s. Optionally proves a speedup changed nothing in the output by diffing
# the build's `html/` tree against a reference build.
#
# MACHINE-SPECIFIC, NOT CI-RUNNABLE: it measures a real project outside this
# repository (waterline's rookery, by default). Run this by hand, on this
# machine, against a project that exists.
#
# Usage: scripts/perf/cold-bench.sh [PROJECT_DIR] [-- extra rheo args]
#   PROJECT_DIR defaults to /home/lox/code/waterline/rookery.
#
# Env knobs:
#   COLD_BENCH_RUNS       number of hyperfine runs (default 5)
#   COLD_BENCH_BIN        rheo binary to time (default: `rheo` on PATH)
#   COLD_BENCH_REFERENCE  directory holding a previous build's `html/` tree;
#                         when set, the new build is diffed against it

set -euo pipefail

die() {
	echo "cold-bench: $*" >&2
	exit 1
}

command -v hyperfine >/dev/null 2>&1 \
	|| die "hyperfine not found on PATH — enter the devShell, or run via \`nix shell nixpkgs#hyperfine\`"

PROJECT="/home/lox/code/waterline/rookery"
EXTRA_ARGS=()
if [[ "${1:-}" != "--" && -n "${1:-}" ]]; then
	PROJECT="$1"
	shift
fi
if [[ "${1:-}" == "--" ]]; then
	shift
	EXTRA_ARGS=("$@")
fi

[[ -d "$PROJECT" ]] || die "project directory does not exist: $PROJECT"

RUNS="${COLD_BENCH_RUNS:-5}"
BIN="${COLD_BENCH_BIN:-rheo}"
command -v "$BIN" >/dev/null 2>&1 || [[ -x "$BIN" ]] \
	|| die "rheo binary not found: $BIN"

TMP="$(mktemp -d -t cold-bench.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

TODAY="$(date +%F)"

hyperfine \
	--runs "$RUNS" \
	--prepare "rm -rf $TMP/out" \
	--export-json "$TMP/hf.json" \
	"$BIN compile $PROJECT --html --input today=$TODAY --build-dir $TMP/out -q ${EXTRA_ARGS[*]}"

if [[ -n "${COLD_BENCH_REFERENCE:-}" ]]; then
	if diff -rq "$COLD_BENCH_REFERENCE/html" "$TMP/out/html" >"$TMP/diff.log" 2>&1; then
		echo "output: byte-identical"
	else
		echo "output: differs"
		cat "$TMP/diff.log"
		exit 1
	fi
fi

if command -v jq >/dev/null 2>&1; then
	MEAN="$(jq -r '.results[0].mean' "$TMP/hf.json")"
	STDDEV="$(jq -r '.results[0].stddev' "$TMP/hf.json")"
	echo "cold-bench: $PROJECT mean ${MEAN}s ± ${STDDEV}s over $RUNS runs"
fi

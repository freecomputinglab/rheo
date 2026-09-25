#!/usr/bin/env bash
#
# watch-bench.sh — measures rheo watch's rebuild latency on a real project.
#
# What it measures: a cold `rheo compile`, then a `rheo watch` session driven
# through three scripted edits (unchanged bytes, a leaf vertebra body edit, an
# asset-only change), three repeats each, reporting the per-rebuild wall time
# rheo's own INFO summary line gives ("N page(s), ... in <dur>"). Per-edit
# rebuild latency is what this harness optimises for; the cold number is
# recorded for reference only and may regress in the rebuild's favour.
#
# MACHINE-SPECIFIC, NOT CI-RUNNABLE: it measures a real project outside this
# repository (waterline's rookery, by default) and needs a `rheo` binary
# built from THIS checkout — `rheo watch --timings` is not yet in any
# released rheo, so the `rheo` on PATH lacks it. There is no synthetic
# fixture standing in for the real project; run this by hand, on this
# machine, against a project that exists.
#
# Usage: scripts/perf/watch-bench.sh [PROJECT_DIR]
#   PROJECT_DIR defaults to /home/lox/code/waterline/rookery.
#
# Every file this script edits under PROJECT_DIR is restored on exit,
# including on failure or Ctrl-C.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROJECT="${1:-/home/lox/code/waterline/rookery}"
: "${CARGO_TARGET_DIR:=/home/lox/.cargo-target}"
export CARGO_TARGET_DIR
BIN="$CARGO_TARGET_DIR/release/rheo"

STARTUP_TIMEOUT_S="${WATCH_BENCH_STARTUP_TIMEOUT:-120}"
EDIT_TIMEOUT_S="${WATCH_BENCH_EDIT_TIMEOUT:-60}"
REPEATS=3

die() {
	echo "watch-bench: $*" >&2
	exit 1
}

[[ -d "$PROJECT" ]] || die "project directory does not exist: $PROJECT"

echo "watch-bench: building release binary from $REPO_ROOT (CARGO_TARGET_DIR=$CARGO_TARGET_DIR)" >&2
( cd "$REPO_ROOT" && cargo build --release ) || die "cargo build --release failed"
[[ -x "$BIN" ]] || die "release binary not found at $BIN after build"

"$BIN" watch --help 2>&1 | grep -q -- '--timings' \
	|| die "$BIN watch --help has no --timings flag — this binary predates wl-extends-timings-to-rheo-watch, rebuild from a checkout that has it"

TMPROOT="$(mktemp -d -t watch-bench.XXXXXX)"
BACKUP_DIR="$TMPROOT/backups"
mkdir -p "$BACKUP_DIR"
LOGFILE="$TMPROOT/watch.log"
WATCH_PID=""
VERTEBRA_FILE=""
ASSET_FILE=""
CLEANED_UP=0

cleanup() {
	[[ "$CLEANED_UP" -eq 1 ]] && return
	CLEANED_UP=1
	if [[ -n "$WATCH_PID" ]] && kill -0 "$WATCH_PID" 2>/dev/null; then
		kill "$WATCH_PID" 2>/dev/null || true
		wait "$WATCH_PID" 2>/dev/null || true
	fi
	if [[ -n "$VERTEBRA_FILE" && -f "$BACKUP_DIR/vertebra" ]]; then
		cp "$BACKUP_DIR/vertebra" "$VERTEBRA_FILE"
	fi
	if [[ -n "$ASSET_FILE" && -f "$BACKUP_DIR/asset" ]]; then
		cp "$BACKUP_DIR/asset" "$ASSET_FILE"
	fi
	rm -rf "$TMPROOT"
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

find_vertebra() {
	local project="$1"
	if [[ -f "$project/index.typ" ]]; then
		echo "$project/index.typ"
		return
	fi
	find "$project" -name '*.typ' \
		-not -path '*/_lib/*' -not -path '*/build/*' \
		-not -name 'template.typ' -not -name 'prelude.typ' -not -name 'index.typ' \
		2>/dev/null | head -n1
}

find_asset() {
	local project="$1"
	local dir="$project/static"
	[[ -d "$dir" ]] || dir="$project"
	local f
	f=$(find "$dir" -type f \( -iname '*.pdf' -o -iname '*.mp4' \) -not -path '*/build/*' 2>/dev/null | head -n1)
	if [[ -z "$f" ]]; then
		f=$(find "$dir" -type f -not -path '*/build/*' 2>/dev/null | head -n1)
	fi
	echo "$f"
}

VERTEBRA_FILE="$(find_vertebra "$PROJECT")"
[[ -n "$VERTEBRA_FILE" && -f "$VERTEBRA_FILE" ]] || die "no leaf vertebra (*.typ) found under $PROJECT"
ASSET_FILE="$(find_asset "$PROJECT")"
[[ -n "$ASSET_FILE" && -f "$ASSET_FILE" ]] || die "no asset file found under $PROJECT/static (or $PROJECT)"

cp "$VERTEBRA_FILE" "$BACKUP_DIR/vertebra"
cp "$ASSET_FILE" "$BACKUP_DIR/asset"

TODAY="$(date +%Y-%m-%d)"

# Duration strings from rheo's summary line are "635ms" or "12.3s" — normalise
# to whole milliseconds so scenarios can be compared and diffed as numbers.
ms_from_duration() {
	local d="$1"
	if [[ "$d" == *ms ]]; then
		echo "${d%ms}"
	else
		awk -v s="${d%s}" 'BEGIN { printf "%.0f", s * 1000 }'
	fi
}

# Extracts the "in <dur>" clause from a rheo INFO summary line.
duration_from_line() {
	local line="$1"
	local dur
	dur="$(grep -oE 'in [0-9]+(\.[0-9]+)?(ms|s)' <<<"$line" | head -n1 | awk '{print $2}')"
	[[ -n "$dur" ]] || die "could not find a duration in summary line: $line"
	ms_from_duration "$dur"
}

# Waits for a new INFO summary line to appear in $LOGFILE after line
# $start_lines, up to $timeout_s seconds. Prints the line and returns 0, or
# returns 1 on timeout — a rebuild that never fires is a harness failure, not
# a number to report.
wait_for_summary() {
	local start_lines="$1" timeout_s="$2"
	local max_iters=$((timeout_s * 5))
	local i cur_lines line
	for ((i = 0; i < max_iters; i++)); do
		cur_lines="$(wc -l <"$LOGFILE" 2>/dev/null || echo 0)"
		if ((cur_lines > start_lines)); then
			line="$(tail -n "+$((start_lines + 1))" "$LOGFILE" | grep -E 'INFO.*page\(s\).*in ' | tail -n1 || true)"
			if [[ -n "$line" ]]; then
				printf '%s\n' "$line"
				return 0
			fi
		fi
		sleep 0.2
	done
	return 1
}

echo "watch-bench: project=$PROJECT rheo=$("$BIN" --version | awk '{print $2}') date=$TODAY"
echo "watch-bench: vertebra=$VERTEBRA_FILE asset=$ASSET_FILE"

# --- cold compile -----------------------------------------------------------

COLD_BUILD_DIR="$TMPROOT/cold-build"
COLD_LOG="$TMPROOT/cold.log"
"$BIN" compile "$PROJECT" --html --build-dir "$COLD_BUILD_DIR" --input "today=$TODAY" >/dev/null 2>"$COLD_LOG" \
	|| die "cold compile failed — see $COLD_LOG"
COLD_LINE="$(grep -E 'INFO.*page\(s\).*in ' "$COLD_LOG" | tail -n1 || true)"
[[ -n "$COLD_LINE" ]] || die "cold compile produced no summary line — see $COLD_LOG"
COLD_MS="$(duration_from_line "$COLD_LINE")"

# --- watch session -----------------------------------------------------------

WATCH_BUILD_DIR="$TMPROOT/watch-build"
"$BIN" watch "$PROJECT" --html --build-dir "$WATCH_BUILD_DIR" --input "today=$TODAY" >/dev/null 2>"$LOGFILE" &
WATCH_PID=$!

wait_for_summary 0 "$STARTUP_TIMEOUT_S" >/dev/null \
	|| die "watch session's initial compile never produced a summary line within ${STARTUP_TIMEOUT_S}s — see $LOGFILE"

run_scenario() {
	local name="$1" edit_fn="$2"
	local -a runs_ms=()
	local start_lines line ms
	for ((run = 1; run <= REPEATS; run++)); do
		start_lines="$(wc -l <"$LOGFILE")"
		"$edit_fn" "$run"
		if ! line="$(wait_for_summary "$start_lines" "$EDIT_TIMEOUT_S")"; then
			die "scenario=$name run=$run: no rebuild fired within ${EDIT_TIMEOUT_S}s — the watcher ignored the edit, coalesced it away, or hit a compile error; see $LOGFILE"
		fi
		ms="$(duration_from_line "$line")"
		runs_ms+=("$ms")
	done
	local sorted
	sorted=($(printf '%s\n' "${runs_ms[@]}" | sort -n))
	echo "scenario=$name runs_ms=$(
		IFS=,
		echo "${runs_ms[*]}"
	) median_ms=${sorted[1]}"
}

edit_touch() {
	touch "$VERTEBRA_FILE"
}

edit_vertebra() {
	local run="$1"
	echo "// watch-bench edit $run" >>"$VERTEBRA_FILE"
}

edit_asset() {
	local run="$1"
	printf '\0' >>"$ASSET_FILE"
	: "$run"
}

RESULT_TOUCH="$(run_scenario touch edit_touch)"
cp "$BACKUP_DIR/vertebra" "$VERTEBRA_FILE"

RESULT_VERTEBRA="$(run_scenario vertebra-edit edit_vertebra)"
cp "$BACKUP_DIR/vertebra" "$VERTEBRA_FILE"

RESULT_ASSET="$(run_scenario asset-edit edit_asset)"
cp "$BACKUP_DIR/asset" "$ASSET_FILE"

kill "$WATCH_PID" 2>/dev/null || true
wait "$WATCH_PID" 2>/dev/null || true
WATCH_PID=""

echo "cold: ${COLD_MS}ms — $COLD_LINE"
echo "$RESULT_TOUCH"
echo "$RESULT_VERTEBRA"
echo "$RESULT_ASSET"

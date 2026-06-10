use std::process::Command;

/// Build a `Command` that invokes the rheo CLI.
///
/// - If `RHEO_BIN` is set: returns `Command::new(<RHEO_BIN>)` — caller appends
///   subcommand args directly (no `cargo run` prefix).
/// - Else: returns `Command::new("cargo")` with `run` + optional
///   `--manifest-path <RHEO_MANIFEST>` + `-p rheo --`, reproducing the
///   current monorepo `cargo run -p rheo --` behaviour.
pub fn rheo_cli_command() -> Command {
    if let Ok(bin) = std::env::var("RHEO_BIN") {
        Command::new(bin)
    } else {
        let mut cmd = Command::new("cargo");
        cmd.arg("run");
        if let Ok(manifest) = std::env::var("RHEO_MANIFEST") {
            cmd.args(["--manifest-path", &manifest]);
        }
        cmd.args(["-p", "rheo", "--"]);
        cmd
    }
}

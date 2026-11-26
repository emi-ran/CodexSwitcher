//! Console command line of CodexSwitcher: `codexswitcher-cli <command>`.
//!
//! This binary keeps the default console subsystem (no `windows_subsystem` attribute), so shells
//! wait for it and receive its exit code. It never starts the GUI; all arguments, including
//! none, go through the same parser as the command line of `codexswitcher.exe`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(codexswitcher_lib::cli_exit_code(&args));
}

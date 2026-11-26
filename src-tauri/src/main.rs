#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = codexswitcher_lib::command_exit_code(&args) {
        std::process::exit(code);
    }
    #[cfg(windows)]
    if let Some(mode) = std::env::args()
        .find(|arg| arg == "--uninstall-cleanup" || arg == "--uninstall-cleanup-silent")
    {
        std::process::exit(codexswitcher_lib::windows_uninstall_cleanup(
            mode == "--uninstall-cleanup",
        ));
    }
    codexswitcher_lib::run();
}

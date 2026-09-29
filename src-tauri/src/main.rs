#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
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

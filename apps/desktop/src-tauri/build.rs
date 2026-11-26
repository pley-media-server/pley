fn main() {
    // The app's own commands each need a permission, which only the
    // picker's capability grants (capabilities/default.json): pages of
    // servers get none of them but `theme_picked` (capabilities/server.json).
    let commands = tauri_build::AppManifest::new().commands(&[
        "saved_servers",
        "save_server",
        "forget_server",
        "check_server",
        "discover",
        "open_server",
        "downloads",
        "play_download",
        "show_download",
        "remove_download",
        "retry_download",
        "theme_picked",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(commands)).expect("tauri build");
}

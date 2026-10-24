fn main() {
    // Eigene Befehle als Berechtigungen, damit sie pro Fenster/Herkunft freigegeben werden können
    // (capabilities/default.json für die Startseite, capabilities/remote.json für die Web-App).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "splash_state",
            "retry",
            "open_logs",
            "open_folder",
            "quit",
            "set_lang",
            "save_download",
            "reveal_download",
            "install",
            "cancel_install",
            "choose_home",
            "open_storage",
            "close_storage",
            "storage_summary",
            "delete_model",
            "clear_cache",
            "reset_all",
            "change_models",
        ]),
    ))
    .expect("tauri-build fehlgeschlagen");
}

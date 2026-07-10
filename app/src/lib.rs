//! `billz-app` — the thin Tauri shell. Real `#[tauri::command]`s that delegate into
//! `billz-core` arrive in bead `cwt.2`; Wave A is just the window + one bridge smoke-test.

/// Trivial bridge command: proves the Svelte -> Rust `invoke` path is wired.
/// Deliberately does NOT touch `billz-core` (that boundary is exercised in cwt.2).
#[tauri::command]
fn app_name() -> &'static str {
    "billz"
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_name])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

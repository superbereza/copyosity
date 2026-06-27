use crate::db::{AppSettings, AppSettingsUpdate, ClickAction, ClipboardEntry, Collection, Database, ExcludedApp, ModelCatalog};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct UpdateCheckResult {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub release_url: String,
    pub notes: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
}

fn parse_semver(input: &str) -> Option<(u32, u32, u32)> {
    let mut s = input.trim();
    if let Some(stripped) = s.strip_prefix('v').or_else(|| s.strip_prefix('V')) {
        s = stripped;
    }
    // Drop prerelease / build metadata for comparison
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.splitn(3, '.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

#[tauri::command]
pub fn check_for_update() -> Result<UpdateCheckResult, String> {
    let url = "https://api.github.com/repos/superbereza/copyosity/releases/latest";
    let resp = ureq::get(url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "Copyosity-Updater")
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("Network error: {}", e))?;

    let release: GhRelease = resp
        .into_json()
        .map_err(|e| format!("Failed to parse GitHub response: {}", e))?;

    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest_clean = release
        .tag_name
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .to_string();

    let has_update = match (parse_semver(&current), parse_semver(&latest_clean)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    };

    Ok(UpdateCheckResult {
        current,
        latest: latest_clean,
        has_update,
        release_url: release.html_url,
        notes: release.body.unwrap_or_default(),
    })
}

#[cfg(target_os = "macos")]
fn simulate_paste() {
    std::thread::sleep(std::time::Duration::from_millis(150));

    unsafe {
        // CoreGraphics FFI — CGEvent for Cmd+V
        type CGEventSourceRef = *mut std::ffi::c_void;
        type CGEventRef = *mut std::ffi::c_void;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventCreateKeyboardEvent(source: CGEventSourceRef, keycode: u16, key_down: bool) -> CGEventRef;
            fn CGEventSetFlags(event: CGEventRef, flags: u64);
            fn CGEventPost(tap: u32, event: CGEventRef);
            fn CFRelease(cf: *mut std::ffi::c_void);
        }

        const K_CG_EVENT_FLAG_COMMAND: u64 = 0x00100000;
        const K_CG_HID_EVENT_TAP: u32 = 0;
        const K_V_KEYCODE: u16 = 9;

        let event_down = CGEventCreateKeyboardEvent(std::ptr::null_mut(), K_V_KEYCODE, true);
        let event_up = CGEventCreateKeyboardEvent(std::ptr::null_mut(), K_V_KEYCODE, false);

        if !event_down.is_null() && !event_up.is_null() {
            CGEventSetFlags(event_down, K_CG_EVENT_FLAG_COMMAND);
            CGEventSetFlags(event_up, K_CG_EVENT_FLAG_COMMAND);
            CGEventPost(K_CG_HID_EVENT_TAP, event_down);
            CGEventPost(K_CG_HID_EVENT_TAP, event_up);
            CFRelease(event_down);
            CFRelease(event_up);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn simulate_paste() {}
use crate::ollama;
use arboard::{Clipboard, ImageData};
use base64::Engine;
use image::GenericImageView;
use std::borrow::Cow;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub fn get_entries(
    db: State<'_, Arc<Database>>,
    limit: Option<i64>,
    offset: Option<i64>,
    collection_id: Option<i64>,
    pinned_only: Option<bool>,
    search: Option<String>,
) -> Result<Vec<ClipboardEntry>, String> {
    db.get_entries(
        limit.unwrap_or(50),
        offset.unwrap_or(0),
        collection_id,
        pinned_only.unwrap_or(false),
        search.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_entry(db: State<'_, Arc<Database>>, id: i64) -> Result<(), String> {
    db.delete_entry(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pin_entry(db: State<'_, Arc<Database>>, id: i64, pinned: bool) -> Result<(), String> {
    db.pin_entry(id, pinned).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_entry_collection(
    db: State<'_, Arc<Database>>,
    entry_id: i64,
    collection_id: Option<i64>,
) -> Result<(), String> {
    db.set_collection(entry_id, collection_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_collections(db: State<'_, Arc<Database>>) -> Result<Vec<Collection>, String> {
    db.get_collections().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_collection(
    db: State<'_, Arc<Database>>,
    name: String,
    color: Option<String>,
) -> Result<i64, String> {
    db.create_collection(&name, color.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_collection(db: State<'_, Arc<Database>>, id: i64) -> Result<(), String> {
    db.delete_collection(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(db: State<'_, Arc<Database>>) -> Result<(), String> {
    db.clear_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::hide_panel(&app);
    Ok(())
}

#[tauri::command]
pub fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    // If settings window already exists, just focus it
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    // Create a new settings window
    let builder = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("/settings".into()),
    )
    .title("Copyosity Settings")
    .inner_size(560.0, 720.0)
    .theme(Some(tauri::Theme::Dark))
    .resizable(false)
    .center();

    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay);

    let _window = builder.build().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn quit_app(_app: tauri::AppHandle) -> Result<(), String> {
    std::process::exit(0);
}

#[tauri::command]
pub fn get_app_settings(db: State<'_, Arc<Database>>) -> Result<AppSettings, String> {
    db.get_app_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_app_settings(
    db: State<'_, Arc<Database>>,
    ollama_model: Option<String>,
    retention_days: Option<i64>,
    main_shortcut: Option<String>,
    show_in_dock: Option<bool>,
    single_click_action: Option<ClickAction>,
    double_click_action: Option<ClickAction>,
) -> Result<AppSettings, String> {
    let settings = db
        .update_app_settings(AppSettingsUpdate {
            ollama_model: ollama_model.as_deref(),
            retention_days,
            main_shortcut: main_shortcut.as_deref(),
            show_in_dock,
            single_click_action,
            double_click_action,
        })
        .map_err(|e| e.to_string())?;

    ollama::set_active_model(&settings.ollama_model);
    ollama::ensure_runtime();

    db.cleanup_old_entries(settings.retention_days)
        .map_err(|e| e.to_string())?;

    Ok(settings)
}

#[tauri::command]
pub fn get_model_catalog() -> Result<ModelCatalog, String> {
    Ok(ollama::model_catalog())
}

#[tauri::command]
pub fn get_excluded_apps(db: State<'_, Arc<Database>>) -> Result<Vec<ExcludedApp>, String> {
    db.get_excluded_apps().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_excluded_app(db: State<'_, Arc<Database>>, bundle_id: String) -> Result<(), String> {
    db.add_excluded_app(&bundle_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_excluded_app(db: State<'_, Arc<Database>>, id: i64) -> Result<(), String> {
    db.remove_excluded_app(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_frontmost_app_to_excluded(
    db: State<'_, Arc<Database>>,
) -> Result<Option<String>, String> {
    let app_name = crate::clipboard_monitor::get_frontmost_app();
    if let Some(app_name) = &app_name {
        db.add_excluded_app(app_name).map_err(|e| e.to_string())?;
    }
    Ok(app_name)
}

#[tauri::command]
pub fn retag_entry(
    app: tauri::AppHandle,
    db: State<'_, Arc<Database>>,
    entry_id: i64,
) -> Result<(), String> {
    let Some(text) = db.get_entry_text(entry_id).map_err(|e| e.to_string())? else {
        return Ok(());
    };

    match ollama::tag_text(&text) {
        Some(tags) => db.set_entry_tags(entry_id, &tags).map_err(|e| e.to_string())?,
        None => db
            .set_entry_tag_state(entry_id, "skipped")
            .map_err(|e| e.to_string())?,
    }

    let _ = app.emit("entry-tagged", entry_id);
    Ok(())
}

#[tauri::command]
pub fn copy_entry(db: State<'_, Arc<Database>>, entry_id: i64) -> Result<(), String> {
    let Some(entry) = db.get_entry_by_id(entry_id).map_err(|e| e.to_string())? else {
        return Ok(());
    };

    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    match entry.content_type.as_str() {
        "text" => {
            if let Some(text) = entry.text_content {
                clipboard.set_text(text).map_err(|e| e.to_string())?;
            }
        }
        "image" => {
            let encoded = entry
                .image_data
                .or(entry.image_thumb)
                .ok_or_else(|| "Image data is missing".to_string())?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| e.to_string())?;
            let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
            let rgba = image.to_rgba8();
            let (width, height) = image.dimensions();
            clipboard
                .set_image(ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: Cow::Owned(rgba.into_raw()),
                })
                .map_err(|e| e.to_string())?;
        }
        _ => {}
    }

    Ok(())
}

#[tauri::command]
pub fn activate_entry(app: tauri::AppHandle, db: State<'_, Arc<Database>>, entry_id: i64) -> Result<(), String> {
    let Some(entry) = db.get_entry_by_id(entry_id).map_err(|e| e.to_string())? else {
        return Ok(());
    };

    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    match entry.content_type.as_str() {
        "text" => {
            if let Some(text) = entry.text_content {
                clipboard.set_text(text).map_err(|e| e.to_string())?;
            }
        }
        "image" => {
            let encoded = entry
                .image_data
                .or(entry.image_thumb)
                .ok_or_else(|| "Image data is missing".to_string())?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| e.to_string())?;
            let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
            let rgba = image.to_rgba8();
            let (width, height) = image.dimensions();
            clipboard
                .set_image(ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: Cow::Owned(rgba.into_raw()),
                })
                .map_err(|e| e.to_string())?;
        }
        _ => return Ok(()),
    }

    crate::hide_panel(&app);
    simulate_paste();

    Ok(())
}

#[tauri::command]
pub fn paste_entry(app: tauri::AppHandle, text: String) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(&text).map_err(|e| e.to_string())?;

    crate::hide_panel(&app);
    simulate_paste();

    Ok(())
}

/// Silent check — returns trusted state without prompting.
#[tauri::command]
pub fn check_accessibility() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        unsafe {
            #[link(name = "ApplicationServices", kind = "framework")]
            extern "C" {
                fn AXIsProcessTrusted() -> bool;
            }
            return Ok(AXIsProcessTrusted());
        }
    }
    #[cfg(not(target_os = "macos"))]
    Ok(true)
}

/// Prompts the system dialog and returns the current trusted state.
#[tauri::command]
pub fn request_accessibility() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        unsafe {
            #[link(name = "ApplicationServices", kind = "framework")]
            extern "C" {
                fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
            }

            use objc::{msg_send, sel, sel_impl};
            use objc::runtime::Object;

            let key: *mut Object = msg_send![
                objc::runtime::Class::get("NSString").unwrap(),
                stringWithUTF8String: b"AXTrustedCheckOptionPrompt\0".as_ptr()
            ];
            let yes: *mut Object = msg_send![
                objc::runtime::Class::get("NSNumber").unwrap(),
                numberWithBool: true
            ];
            let dict: *mut Object = msg_send![
                objc::runtime::Class::get("NSDictionary").unwrap(),
                dictionaryWithObject: yes forKey: key
            ];

            let trusted = AXIsProcessTrustedWithOptions(dict as *const _);
            return Ok(trusted);
        }
    }
    #[cfg(not(target_os = "macos"))]
    Ok(true)
}

#[tauri::command]
pub fn check_ollama_status() -> Result<ollama::OllamaStatus, String> {
    Ok(ollama::check_status())
}

#[tauri::command]
pub fn start_ollama_server() -> Result<bool, String> {
    Ok(ollama::try_start_server())
}

#[tauri::command]
pub fn pull_ollama_model(app: tauri::AppHandle) -> Result<(), String> {
    std::thread::spawn(move || {
        let result = ollama::try_pull_model(Some(&app));
        let _ = app.emit("ollama-pull-done", result);
    });
    Ok(())
}

#[tauri::command]
pub fn unload_ollama_model() -> Result<bool, String> {
    Ok(ollama::unload_model())
}

#[tauri::command]
pub fn test_ollama_tagging() -> Result<Option<Vec<String>>, String> {
    Ok(ollama::test_tagging())
}

#[tauri::command]
pub fn rebind_main_shortcut(app: tauri::AppHandle) -> Result<String, String> {
    crate::register_main_shortcut(&app)
}

#[tauri::command]
pub fn restart_app_with_settings_open(
    app: tauri::AppHandle,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.set_setting("reopen_settings_on_launch", "1")
        .map_err(|e| e.to_string())?;

    // In dev builds `app.restart()` re-execs the binary, kills the
    // `npm run tauri dev` parent and orphans Vite. Bypass Tauri's prevent_exit
    // via std::process::exit so it actually quits.
    #[cfg(debug_assertions)]
    {
        let _ = app;
        std::process::exit(0);
    }

    #[cfg(not(debug_assertions))]
    {
        app.restart();
    }
}

#[tauri::command]
pub fn set_main_banner_shown(app: tauri::AppHandle, shown: bool) -> Result<(), String> {
    crate::set_banner_shown_internal(&app, shown);
    Ok(())
}


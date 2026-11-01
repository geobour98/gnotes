//! gnotes desktop app: exposes the encrypted vault to the UI through a small,
//! explicit set of commands. The UI never touches the filesystem directly.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gnotes_core::attachments::{self, AttachmentMeta};
use gnotes_core::{Note, NoteMeta, Tree, Vault};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};
use zeroize::Zeroizing;

type CmdResult<T> = Result<T, String>;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Config {
    last_vault: Option<String>,
    auto_lock_minutes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config { last_vault: None, auto_lock_minutes: 10 }
    }
}

struct AppState {
    vault: Mutex<Option<Vault>>,
    last_activity: Mutex<Instant>,
    config: Mutex<Config>,
    /// Set once we are really quitting, so the next close isn't intercepted.
    quitting: AtomicBool,
    /// Bumped on every close request; lets a stale fallback timer notice
    /// that a newer request superseded it.
    close_gen: AtomicU64,
    /// The UI confirmed it received the close request and is handling it.
    close_acked: AtomicBool,
}

impl AppState {
    /// Drops the vault: closes the database and zeroizes the key.
    fn lock_vault(&self) -> bool {
        self.vault.lock().unwrap().take().is_some()
    }

    fn touch(&self) {
        *self.last_activity.lock().unwrap() = Instant::now();
    }

    /// Runs `f` against the unlocked vault and records activity.
    fn with_vault<T>(&self, f: impl FnOnce(&mut Vault) -> gnotes_core::Result<T>) -> CmdResult<T> {
        self.touch();
        let mut guard = self.vault.lock().unwrap();
        let vault = guard.as_mut().ok_or("vault is locked")?;
        f(vault).map_err(|e| e.to_string())
    }
}

fn config_path(app: &AppHandle) -> CmdResult<PathBuf> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("config.json"))
}

fn load_config(app: &AppHandle) -> Config {
    config_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(app: &AppHandle, cfg: &Config) -> CmdResult<()> {
    let p = config_path(app)?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(p, serde_json::to_string_pretty(cfg).unwrap()).map_err(|e| e.to_string())
}

/// UTC timestamp `YYYYMMDD-HHMMSS` for backup file names.
fn timestamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

// ------------------------------------------------------------- vault lifecycle

#[derive(Serialize)]
struct Status {
    unlocked: bool,
    /// Vault to suggest on the unlock screen.
    vault_path: String,
    vault_exists: bool,
    auto_lock_minutes: u64,
}

#[tauri::command]
fn vault_status(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Status> {
    let cfg = state.config.lock().unwrap().clone();
    let unlocked_path = state.vault.lock().unwrap().as_ref().map(|v| v.path().display().to_string());
    let vault_path = match (unlocked_path.clone(), cfg.last_vault) {
        (Some(p), _) | (None, Some(p)) => p,
        (None, None) => app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("notes.gnotes")
            .display()
            .to_string(),
    };
    Ok(Status {
        unlocked: unlocked_path.is_some(),
        vault_exists: PathBuf::from(&vault_path).exists(),
        vault_path,
        auto_lock_minutes: cfg.auto_lock_minutes,
    })
}

async fn open_or_create(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    password: String,
    create: bool,
) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    if create && password.chars().count() < 8 {
        return Err("password must be at least 8 characters".into());
    }
    let p = PathBuf::from(path.trim());
    if !p.is_absolute() {
        return Err("vault path must be absolute".into());
    }
    // Argon2 takes ~0.5 s; keep it off the UI thread.
    let p2 = p.clone();
    let vault = tauri::async_runtime::spawn_blocking(move || {
        if create { Vault::create(&p2, &password) } else { Vault::open(&p2, &password) }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    *state.vault.lock().unwrap() = Some(vault);
    state.touch();
    let cfg = {
        let mut cfg = state.config.lock().unwrap();
        cfg.last_vault = Some(p.display().to_string());
        cfg.clone()
    };
    save_config(&app, &cfg)
}

#[tauri::command]
async fn vault_create(app: AppHandle, state: State<'_, AppState>, path: String, password: String) -> CmdResult<()> {
    open_or_create(app, state, path, password, true).await
}

#[tauri::command]
async fn vault_unlock(app: AppHandle, state: State<'_, AppState>, path: String, password: String) -> CmdResult<()> {
    open_or_create(app, state, path, password, false).await
}

#[tauri::command]
fn vault_lock(state: State<'_, AppState>) {
    state.lock_vault();
}

// ------------------------------------------------------------------ quitting

/// Locks the vault and exits. Called by the UI once pending edits are saved.
fn quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.quitting.store(true, Ordering::SeqCst);
    let _was_unlocked = state.lock_vault();
    #[cfg(debug_assertions)]
    eprintln!("[gnotes] quitting; vault was unlocked: {_was_unlocked}, now locked");
    app.exit(0);
}

/// The window's close button was pressed. While a vault is unlocked, keep
/// the window open and let the UI save first; it then calls `app_quit`.
fn on_close_requested(app: &AppHandle, api: &tauri::CloseRequestApi) {
    let state = app.state::<AppState>();
    if state.quitting.load(Ordering::SeqCst) || state.vault.lock().unwrap().is_none() {
        state.quitting.store(true, Ordering::SeqCst);
        #[cfg(debug_assertions)]
        eprintln!("[gnotes] close with vault locked; exiting");
        return; // nothing decrypted: close normally
    }
    api.prevent_close();
    let generation = state.close_gen.fetch_add(1, Ordering::SeqCst) + 1;
    state.close_acked.store(false, Ordering::SeqCst);
    let _ = app.emit("close-requested", ());

    // If the UI is unresponsive, don't hang: lock and exit anyway.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        let state = app.state::<AppState>();
        if state.close_gen.load(Ordering::SeqCst) == generation && !state.close_acked.load(Ordering::SeqCst) {
            #[cfg(debug_assertions)]
            eprintln!("[gnotes] UI did not answer the close request; forcing lock + exit");
            quit(&app);
        }
    });
}

#[tauri::command]
fn quit_ack(state: State<'_, AppState>) {
    #[cfg(debug_assertions)]
    eprintln!("[gnotes] close request received by UI; saving");
    state.close_acked.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn app_quit(app: AppHandle) {
    quit(&app);
}

/// Called by the UI on user activity so reading (without saving) still
/// counts against the auto-lock timer.
#[tauri::command]
fn activity(state: State<'_, AppState>) {
    state.touch();
}

#[tauri::command]
fn set_auto_lock(app: AppHandle, state: State<'_, AppState>, minutes: u64) -> CmdResult<()> {
    let cfg = {
        let mut cfg = state.config.lock().unwrap();
        cfg.auto_lock_minutes = minutes.min(24 * 60);
        cfg.clone()
    };
    save_config(&app, &cfg)
}

#[tauri::command]
async fn change_password(state: State<'_, AppState>, current: String, new: String) -> CmdResult<()> {
    let (current, new) = (Zeroizing::new(current), Zeroizing::new(new));
    if new.chars().count() < 8 {
        return Err("password must be at least 8 characters".into());
    }
    let path = state.with_vault(|v| Ok(v.path().to_path_buf()))?;
    // Confirm the current password before re-keying.
    tauri::async_runtime::spawn_blocking(move || Vault::open(&path, &current).map(drop))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "current password is wrong".to_string())?;
    state.with_vault(|v| v.change_password(&new))
}

/// Writes a timestamped encrypted backup next to the vault and returns its path.
#[tauri::command]
fn backup_now(state: State<'_, AppState>) -> CmdResult<String> {
    state.with_vault(|v| {
        let src = v.path();
        let dir = src.parent().unwrap_or(src).join("gnotes-backups");
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("vault");
        let dest = dir.join(format!("{stem}-{}.gnotes", timestamp()));
        v.backup_to(&dest)?;
        Ok(dest.display().to_string())
    })
}

// ---------------------------------------------------------------- attachments

/// Decodes `%XX` escapes (used for file names sent in IPC headers).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Adds an attachment from raw bytes (pasted images). The body is the file
/// content; `x-name` / `x-mime` headers carry the metadata.
#[tauri::command]
fn attachment_add(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<AttachmentMeta> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return Err("expected raw file bytes".into());
    };
    let header = |k: &str| request.headers().get(k).and_then(|v| v.to_str().ok()).map(percent_decode);
    let name = header("x-name").unwrap_or_else(|| "pasted".into());
    let mime = header("x-mime");
    state.with_vault(|v| v.add_attachment(&name, mime.as_deref(), data))
}

/// Adds files the user dropped onto the window or picked in a file dialog.
#[tauri::command]
async fn attachment_add_paths(state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<Vec<AttachmentMeta>> {
    let mut out = Vec::new();
    for p in paths {
        let path = PathBuf::from(&p);
        let meta = fs::metadata(&path).map_err(|e| format!("{p}: {e}"))?;
        if !meta.is_file() {
            return Err(format!("{p}: not a file"));
        }
        if meta.len() as usize > attachments::MAX_SIZE {
            return Err(format!("{p}: larger than {} MB", attachments::MAX_SIZE >> 20));
        }
        let data = tauri::async_runtime::spawn_blocking(move || fs::read(path))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{p}: {e}"))?;
        let name = PathBuf::from(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(state.with_vault(|v| v.add_attachment(&name, None, &data))?);
    }
    Ok(out)
}

#[tauri::command]
fn attachment_meta(state: State<'_, AppState>, id: String) -> CmdResult<AttachmentMeta> {
    state.with_vault(|v| v.attachment_meta(&id))
}

/// Writes a decrypted copy of an attachment to a path the user chose.
#[tauri::command]
fn attachment_save(state: State<'_, AppState>, id: String, path: String) -> CmdResult<()> {
    let (_, data) = state.with_vault(|v| v.attachment(&id))?;
    fs::write(&path, data).map_err(|e| format!("{path}: {e}"))
}

/// Serves attachments to the preview at `gnotes://localhost/<id>` (Linux,
/// macOS) or `http://gnotes.localhost/<id>` (Windows) straight from the
/// unlocked vault: decrypted bytes never touch the disk.
fn serve_attachment(app: &AppHandle, path: &str) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{Response, StatusCode, header};
    let id = path.trim_start_matches('/');
    let found = if attachments::is_valid_id(id) {
        let state = app.state::<AppState>();
        let guard = state.vault.lock().unwrap();
        guard.as_ref().and_then(|v| v.attachment(id).ok())
    } else {
        None
    };
    match found {
        Some((meta, data)) => {
            // Only media is rendered inline; anything else is opaque bytes.
            let inline = ["image/", "audio/", "video/"].iter().any(|p| meta.mime.starts_with(p));
            let mime = if inline { meta.mime.as_str() } else { "application/octet-stream" };
            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, "no-store")
                .header("X-Content-Type-Options", "nosniff")
                .header(header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; sandbox")
                .body(data)
                .unwrap()
        }
        None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap(),
    }
}

// ------------------------------------------------------------- notes & folders

#[tauri::command]
fn tree(state: State<'_, AppState>) -> CmdResult<Tree> {
    state.with_vault(|v| v.tree())
}

#[tauri::command]
fn folder_create(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    state.with_vault(|v| v.create_folder(&path))
}

#[tauri::command]
fn folder_rename(state: State<'_, AppState>, old: String, new: String) -> CmdResult<()> {
    state.with_vault(|v| v.rename_folder(&old, &new))
}

#[tauri::command]
fn folder_delete(state: State<'_, AppState>, path: String) -> CmdResult<usize> {
    state.with_vault(|v| v.delete_folder(&path))
}

#[tauri::command]
fn note_create(state: State<'_, AppState>, folder: String, title: Option<String>) -> CmdResult<NoteMeta> {
    state.with_vault(|v| v.create_note(&folder, title.as_deref()))
}

#[tauri::command]
fn note_get(state: State<'_, AppState>, id: String) -> CmdResult<Note> {
    state.with_vault(|v| v.get_note(&id))
}

#[tauri::command]
fn note_save(state: State<'_, AppState>, id: String, body: String) -> CmdResult<i64> {
    state.with_vault(|v| v.save_note(&id, &body))
}

#[tauri::command]
fn note_move(state: State<'_, AppState>, id: String, folder: String, title: String) -> CmdResult<NoteMeta> {
    state.with_vault(|v| v.move_note(&id, &folder, &title))
}

#[tauri::command]
fn note_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.with_vault(|v| v.delete_note(&id))
}

/// Background thread that locks the vault after the configured idle time.
fn spawn_auto_lock(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let state = app.state::<AppState>();
            let minutes = state.config.lock().unwrap().auto_lock_minutes;
            if minutes == 0 {
                continue;
            }
            let idle = state.last_activity.lock().unwrap().elapsed();
            if idle >= Duration::from_secs(minutes * 60) {
                if state.lock_vault() {
                    let _ = app.emit("vault-locked", ());
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("gnotes", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            std::thread::spawn(move || responder.respond(serve_attachment(&app, &path)));
        })
        .setup(|app| {
            let config = load_config(app.handle());
            app.manage(AppState {
                vault: Mutex::new(None),
                last_activity: Mutex::new(Instant::now()),
                config: Mutex::new(config),
                quitting: AtomicBool::new(false),
                close_gen: AtomicU64::new(0),
                close_acked: AtomicBool::new(false),
            });
            spawn_auto_lock(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                on_close_requested(window.app_handle(), api);
            }
        })
        .invoke_handler(tauri::generate_handler![
            vault_status,
            vault_create,
            vault_unlock,
            vault_lock,
            quit_ack,
            app_quit,
            activity,
            set_auto_lock,
            change_password,
            backup_now,
            attachment_add,
            attachment_add_paths,
            attachment_meta,
            attachment_save,
            tree,
            folder_create,
            folder_rename,
            folder_delete,
            note_create,
            note_get,
            note_save,
            note_move,
            note_delete,
        ])
        .build(tauri::generate_context!())
        .expect("error while building gnotes")
        .run(|app, event| {
            // Covers every exit path (e.g. Ctrl+Q from the OS, app.exit):
            // drop the vault so the key is zeroized.
            if let RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.lock_vault();
                }
            }
        });
}

//! Lock the vault when the screen locks or the system goes to sleep (Linux).
//!
//! Listens to systemd-logind on the local system D-Bus (not the network):
//!
//! - `PrepareForSleep(true)` before suspend/hibernate. A "delay" inhibitor
//!   holds the suspend back (max a few seconds, logind's InhibitDelayMaxSec)
//!   until the vault is locked, so the key is wiped before RAM is frozen.
//! - the session's `Lock` signal (`loginctl lock-session`, hypridle, ...).
//! - the session's `LockedHint` turning true (set by GNOME, KDE, hyprlock).
//!
//! Each time, the UI is asked to save pending edits and lock; if it doesn't
//! within [`UI_GRACE`], the vault is locked anyway.

use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedFd, OwnedObjectPath};

use crate::AppState;

const DEST: &str = "org.freedesktop.login1";
const UI_GRACE: Duration = Duration::from_millis(1500);

/// Starts the listeners. Missing logind (non-systemd systems) only means
/// this feature is unavailable; everything else keeps working.
pub fn spawn(app: AppHandle) {
    thread::spawn(move || {
        if let Err(_e) = listen(app) {
            #[cfg(debug_assertions)]
            eprintln!("[gnotes] screen-lock/sleep listener unavailable: {_e}");
        }
    });
}

fn listen(app: AppHandle) -> zbus::Result<()> {
    let conn = Connection::system()?;
    let manager = Proxy::new(&conn, DEST, "/org/freedesktop/login1", "org.freedesktop.login1.Manager")?;

    // "auto" = our session, or the user's graphical session when we were
    // started outside one (e.g. from a systemd app scope).
    let session_path: OwnedObjectPath =
        manager.call("GetSession", &("auto",)).or_else(|_| manager.call("GetSessionByPID", &(std::process::id(),)))?;
    let session = Proxy::new(&conn, DEST, session_path, "org.freedesktop.login1.Session")?;

    {
        let (app, session) = (app.clone(), session.clone());
        thread::spawn(move || {
            let Ok(signals) = session.receive_signal("Lock") else { return };
            for _ in signals {
                lock_now(&app, "screen");
            }
        });
    }
    {
        let (app, session) = (app.clone(), session.clone());
        thread::spawn(move || {
            for change in session.receive_property_changed::<bool>("LockedHint") {
                if change.get().unwrap_or(false) {
                    lock_now(&app, "screen");
                }
            }
        });
    }

    #[cfg(debug_assertions)]
    eprintln!("[gnotes] listening for screen lock and sleep");

    // Sleep: this thread.
    let mut inhibitor = inhibit(&manager);
    for signal in manager.receive_signal("PrepareForSleep")? {
        let going_to_sleep: bool = signal.body().deserialize().unwrap_or(false);
        if going_to_sleep {
            lock_now(&app, "sleep");
            inhibitor = None; // closing the fd lets the suspend proceed
        } else if inhibitor.is_none() {
            inhibitor = inhibit(&manager); // resumed: hold the next one
        }
    }
    Ok(())
}

/// A logind "delay" inhibitor: suspend waits until the fd is closed.
fn inhibit(manager: &Proxy) -> Option<OwnedFd> {
    manager.call("Inhibit", &("sleep", "gnotes", "Lock the vault before sleeping", "delay")).ok()
}

/// Asks the UI to save and lock; locks directly if it doesn't in time.
fn lock_now(app: &AppHandle, reason: &str) {
    let state = app.state::<AppState>();
    if !state.config.lock().unwrap().lock_on_screen_lock || state.quitting.load(Ordering::SeqCst) {
        return;
    }
    if state.vault.lock().unwrap().is_none() {
        return;
    }
    #[cfg(debug_assertions)]
    eprintln!("[gnotes] {reason}: locking vault");
    let _ = app.emit("lock-requested", reason);
    let deadline = Instant::now() + UI_GRACE;
    while Instant::now() < deadline {
        if state.vault.lock().unwrap().is_none() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    if state.lock_vault() {
        #[cfg(debug_assertions)]
        eprintln!("[gnotes] {reason}: UI did not lock in time; locked directly");
        let _ = app.emit("vault-locked", reason);
    }
}

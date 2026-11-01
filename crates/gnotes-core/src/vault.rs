//! The encrypted vault: one SQLCipher file holding folders and notes.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN, SALT_LEN};
use crate::error::{Error, Result};
use crate::paths;

/// Current schema version, stored in `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 2;

const SCHEMA_V1: &str = "
CREATE TABLE folders (
    path TEXT PRIMARY KEY NOT NULL
) WITHOUT ROWID;

CREATE TABLE notes (
    id      TEXT PRIMARY KEY NOT NULL,
    folder  TEXT NOT NULL,
    title   TEXT NOT NULL,
    body    TEXT NOT NULL DEFAULT '',
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    UNIQUE (folder, title)
);
CREATE INDEX notes_folder ON notes(folder);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) WITHOUT ROWID;
";

/// v2: files embedded in notes (images, PDFs, ...), referenced from note
/// bodies as `attachment:<id>`.
const SCHEMA_V2: &str = "
CREATE TABLE attachments (
    id      TEXT PRIMARY KEY NOT NULL,
    name    TEXT NOT NULL,
    mime    TEXT NOT NULL,
    size    INTEGER NOT NULL,
    data    BLOB NOT NULL,
    created INTEGER NOT NULL
);
";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NoteMeta {
    pub id: String,
    pub folder: String,
    pub title: String,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Note {
    pub id: String,
    pub folder: String,
    pub title: String,
    pub body: String,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tree {
    pub folders: Vec<String>,
    pub notes: Vec<NoteMeta>,
}

pub struct Vault {
    pub(crate) conn: Connection,
    path: PathBuf,
    salt: [u8; SALT_LEN],
    key: Zeroizing<[u8; KEY_LEN]>,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn read_salt(path: &Path) -> Result<[u8; SALT_LEN]> {
    let mut f = fs::File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::VaultNotFound,
        _ => Error::Io(e),
    })?;
    let mut salt = [0u8; SALT_LEN];
    f.read_exact(&mut salt).map_err(|_| Error::WrongPassword)?;
    Ok(salt)
}

/// Applies the key and checks it by touching the schema.
fn apply_key(conn: &Connection, key: &[u8; KEY_LEN], salt: &[u8; SALT_LEN]) -> Result<()> {
    let lit = crypto::sqlcipher_key_literal(key, salt);
    let sql = Zeroizing::new(format!("PRAGMA key = \"{}\";", lit.as_str()));
    conn.execute_batch(&sql)?;
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::NotADatabase => {
            Err(Error::WrongPassword)
        }
        Err(e) => Err(e.into()),
    }
}

fn open_keyed(path: &Path, key: &[u8; KEY_LEN], salt: &[u8; SALT_LEN], create: bool) -> Result<Connection> {
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let conn = Connection::open_with_flags(path, flags)?;
    apply_key(&conn, key, salt)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON;")?;
    Ok(conn)
}

fn map_unique(e: rusqlite::Error, what: impl Into<String>) -> Error {
    match &e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation => {
            Error::AlreadyExists(what.into())
        }
        _ => Error::Db(e),
    }
}

impl Vault {
    /// Creates a new, empty vault file. Fails if the file exists.
    pub fn create(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            return Err(Error::VaultExists);
        }
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let salt = crypto::random_salt()?;
        let key = crypto::derive_key(password, &salt)?;

        let result = (|| {
            let conn = open_keyed(&path, &key, &salt, true)?;
            let mut vault = Vault { conn, path: path.clone(), salt, key };
            vault.migrate()?;
            // SQLCipher must have stored our salt in the header, otherwise the
            // vault could never be reopened. Verify instead of trusting.
            if read_salt(&path)? != salt {
                return Err(Error::Crypto("vault header does not contain the expected salt".into()));
            }
            Ok(vault)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&path);
        }
        result
    }

    /// Opens an existing vault with the master password.
    pub fn open(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let salt = read_salt(&path)?;
        let key = crypto::derive_key(password, &salt)?;
        let conn = open_keyed(&path, &key, &salt, false)?;
        let mut vault = Vault { conn, path, salt, key };
        vault.migrate()?;
        // Attachments whose references were removed in an earlier session.
        vault.prune_attachments()?;
        Ok(vault)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::UnsupportedSchema(version));
        }
        if version < 1 {
            let tx = self.conn.transaction()?;
            tx.execute_batch(SCHEMA_V1)?;
            tx.execute_batch("PRAGMA user_version = 1;")?;
            tx.commit()?;
        }
        if version < 2 {
            let tx = self.conn.transaction()?;
            tx.execute_batch(SCHEMA_V2)?;
            tx.execute_batch("PRAGMA user_version = 2;")?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Re-encrypts the vault under a new master password.
    pub fn change_password(&mut self, new_password: &str) -> Result<()> {
        let key = crypto::derive_key(new_password, &self.salt)?;
        let lit = crypto::sqlcipher_key_literal(&key, &self.salt);
        let sql = Zeroizing::new(format!("PRAGMA rekey = \"{}\";", lit.as_str()));
        self.conn.execute_batch(&sql)?;
        self.key = key;
        Ok(())
    }

    /// Writes an encrypted copy of the vault to `dest` (same password) and
    /// verifies that the copy opens and passes an integrity check.
    pub fn backup_to(&self, dest: impl AsRef<Path>) -> Result<()> {
        let dest = dest.as_ref();
        if dest.exists() {
            return Err(Error::AlreadyExists(dest.display().to_string()));
        }
        if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        // We hold &self, so no write is in flight, and with the default
        // rollback journal the file on disk is consistent between commits.
        let tmp = dest.with_extension("partial");
        fs::copy(&self.path, &tmp)?;
        let check = (|| {
            let conn = open_keyed(&tmp, &self.key, &self.salt, false)?;
            let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
            if ok != "ok" {
                return Err(Error::Crypto(format!("backup integrity check failed: {ok}")));
            }
            Ok(())
        })();
        match check {
            Ok(()) => {
                fs::rename(&tmp, dest)?;
                Ok(())
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    // ---------------------------------------------------------------- tree

    pub fn tree(&self) -> Result<Tree> {
        let mut st = self.conn.prepare("SELECT path FROM folders ORDER BY path COLLATE NOCASE")?;
        let folders = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut st = self
            .conn
            .prepare("SELECT id, folder, title, updated FROM notes ORDER BY title COLLATE NOCASE")?;
        let notes = st
            .query_map([], |r| {
                Ok(NoteMeta { id: r.get(0)?, folder: r.get(1)?, title: r.get(2)?, updated: r.get(3)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Tree { folders, notes })
    }

    fn folder_exists(&self, path: &str) -> Result<bool> {
        if path.is_empty() {
            return Ok(true);
        }
        Ok(self
            .conn
            .query_row("SELECT 1 FROM folders WHERE path = ?1", [path], |_| Ok(()))
            .optional()?
            .is_some())
    }

    fn require_folder(&self, path: &str) -> Result<()> {
        if self.folder_exists(path)? {
            Ok(())
        } else {
            Err(Error::NotFound(format!("folder {path:?}")))
        }
    }

    // ------------------------------------------------------------- folders

    /// Creates a folder and any missing parents.
    pub fn create_folder(&mut self, path: &str) -> Result<()> {
        paths::validate_folder(path)?;
        if path.is_empty() || self.folder_exists(path)? {
            return Err(Error::AlreadyExists(format!("folder {path:?}")));
        }
        let tx = self.conn.transaction()?;
        for p in paths::ancestors(path) {
            tx.execute("INSERT OR IGNORE INTO folders(path) VALUES (?1)", [p])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Renames or moves a folder together with everything inside it.
    pub fn rename_folder(&mut self, old: &str, new: &str) -> Result<()> {
        paths::validate_folder(old)?;
        paths::validate_folder(new)?;
        if old.is_empty() || new.is_empty() {
            return Err(Error::InvalidName("cannot rename the root folder".into()));
        }
        if old == new {
            return Ok(());
        }
        self.require_folder(old)?;
        if paths::is_within(new, old) {
            return Err(Error::InvalidName("cannot move a folder into itself".into()));
        }
        if self.folder_exists(new)? {
            return Err(Error::AlreadyExists(format!("folder {new:?}")));
        }
        let tx = self.conn.transaction()?;
        for p in paths::ancestors(paths::parent(new)) {
            tx.execute("INSERT OR IGNORE INTO folders(path) VALUES (?1)", [p])?;
        }
        let n = old.chars().count() as i64;
        let within = "(path = ?1 OR substr(path, 1, ?3 + 1) = ?1 || '/')";
        tx.execute(
            &format!("UPDATE folders SET path = ?2 || substr(path, ?3 + 1) WHERE {within}"),
            params![old, new, n],
        )?;
        tx.execute(
            &format!(
                "UPDATE notes SET folder = ?2 || substr(folder, ?3 + 1) WHERE {}",
                within.replace("path", "folder")
            ),
            params![old, new, n],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a folder, its subfolders and all notes inside. Returns the
    /// number of deleted notes.
    pub fn delete_folder(&mut self, path: &str) -> Result<usize> {
        paths::validate_folder(path)?;
        if path.is_empty() {
            return Err(Error::InvalidName("cannot delete the root folder".into()));
        }
        self.require_folder(path)?;
        let n = path.chars().count() as i64;
        let tx = self.conn.transaction()?;
        let notes = tx.execute(
            "DELETE FROM notes WHERE folder = ?1 OR substr(folder, 1, ?2 + 1) = ?1 || '/'",
            params![path, n],
        )?;
        tx.execute(
            "DELETE FROM folders WHERE path = ?1 OR substr(path, 1, ?2 + 1) = ?1 || '/'",
            params![path, n],
        )?;
        tx.commit()?;
        Ok(notes)
    }

    // --------------------------------------------------------------- notes

    fn title_taken(&self, folder: &str, title: &str, except_id: Option<&str>) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM notes WHERE folder = ?1 AND title = ?2 AND id IS NOT ?3",
                params![folder, title, except_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Creates an empty note. Without a title it becomes "Untitled",
    /// "Untitled 2", ... whichever is free.
    pub fn create_note(&mut self, folder: &str, title: Option<&str>) -> Result<NoteMeta> {
        paths::validate_folder(folder)?;
        self.require_folder(folder)?;
        let title = match title {
            Some(t) => {
                paths::validate_name(t)?;
                t.to_string()
            }
            None => {
                let mut i = 1;
                loop {
                    let t = if i == 1 { "Untitled".to_string() } else { format!("Untitled {i}") };
                    if !self.title_taken(folder, &t, None)? {
                        break t;
                    }
                    i += 1;
                }
            }
        };
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        self.conn
            .execute(
                "INSERT INTO notes(id, folder, title, body, created, updated) VALUES (?1, ?2, ?3, '', ?4, ?4)",
                params![id, folder, title, now],
            )
            .map_err(|e| map_unique(e, format!("note {:?}", paths::join(folder, &title))))?;
        Ok(NoteMeta { id, folder: folder.to_string(), title, updated: now })
    }

    pub fn get_note(&self, id: &str) -> Result<Note> {
        self.conn
            .query_row(
                "SELECT id, folder, title, body, created, updated FROM notes WHERE id = ?1",
                [id],
                |r| {
                    Ok(Note {
                        id: r.get(0)?,
                        folder: r.get(1)?,
                        title: r.get(2)?,
                        body: r.get(3)?,
                        created: r.get(4)?,
                        updated: r.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("note {id}")))
    }

    /// Saves a note's body. Returns the new `updated` timestamp.
    pub fn save_note(&mut self, id: &str, body: &str) -> Result<i64> {
        let now = now_ms();
        let n = self
            .conn
            .execute("UPDATE notes SET body = ?2, updated = ?3 WHERE id = ?1", params![id, body, now])?;
        if n == 0 {
            return Err(Error::NotFound(format!("note {id}")));
        }
        Ok(now)
    }

    /// Renames and/or moves a note.
    pub fn move_note(&mut self, id: &str, folder: &str, title: &str) -> Result<NoteMeta> {
        paths::validate_folder(folder)?;
        paths::validate_name(title)?;
        self.require_folder(folder)?;
        if self.title_taken(folder, title, Some(id))? {
            return Err(Error::AlreadyExists(format!("note {:?}", paths::join(folder, title))));
        }
        let now = now_ms();
        let n = self.conn.execute(
            "UPDATE notes SET folder = ?2, title = ?3, updated = ?4 WHERE id = ?1",
            params![id, folder, title, now],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("note {id}")));
        }
        Ok(NoteMeta { id: id.to_string(), folder: folder.to_string(), title: title.to_string(), updated: now })
    }

    pub fn delete_note(&mut self, id: &str) -> Result<()> {
        let n = self.conn.execute("DELETE FROM notes WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(Error::NotFound(format!("note {id}")));
        }
        Ok(())
    }
}

//! Attachments: files stored encrypted inside the vault and referenced from
//! note bodies as `attachment:<id>` (e.g. `![photo](attachment:<id>)`).

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::vault::{Vault, now_ms};

/// URL scheme used inside note bodies.
pub const SCHEME: &str = "attachment:";
pub const MAX_SIZE: usize = 100 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AttachmentMeta {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub created: i64,
}

/// Attachment ids are lowercase hyphenated UUIDs.
pub fn is_valid_id(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        })
}

/// Finds `attachment:<id>` references in a note body as (byte range, id).
pub fn find_refs(body: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = body[from..].find(SCHEME) {
        let start = from + pos;
        let id_start = start + SCHEME.len();
        let id = body.get(id_start..id_start + 36).unwrap_or("");
        if is_valid_id(id) {
            out.push((start..id_start + 36, id));
            from = id_start + 36;
        } else {
            from = id_start;
        }
    }
    out
}

/// Makes a user-supplied file name safe to store and to export.
pub fn sanitize_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String =
        base.chars().map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '-' } else { c }).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).to_string();
    if cleaned.is_empty() { "file".to_string() } else { cleaned.chars().take(200).collect() }
}

/// A plain `type/subtype` (no parameters, quotes or whitespace): it ends up
/// in HTTP headers and in exported HTML, so nothing else is accepted.
fn is_valid_mime(m: &str) -> bool {
    let token = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b));
    m.len() < 100 && m.split_once('/').is_some_and(|(a, b)| token(a) && token(b))
}

/// MIME type from the file extension.
pub fn mime_for_name(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "zip" => "application/zip",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// Stores a file (usable inside a transaction). MIME comes from the name
/// when not given.
pub(crate) fn insert(conn: &Connection, name: &str, mime: Option<&str>, data: &[u8]) -> Result<AttachmentMeta> {
    if data.len() > MAX_SIZE {
        return Err(Error::TooLarge(format!("{name} ({} MB, limit {} MB)", data.len() >> 20, MAX_SIZE >> 20)));
    }
    let name = sanitize_name(name);
    let mime = match mime.map(|m| m.trim().to_ascii_lowercase()) {
        Some(m) if is_valid_mime(&m) => m,
        _ => mime_for_name(&name).to_string(),
    };
    let meta =
        AttachmentMeta { id: uuid::Uuid::new_v4().to_string(), name, mime, size: data.len() as i64, created: now_ms() };
    conn.execute(
        "INSERT INTO attachments(id, name, mime, size, data, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![meta.id, meta.name, meta.mime, meta.size, data, meta.created],
    )?;
    Ok(meta)
}

impl Vault {
    /// Stores a file. The MIME type is derived from the name when not given.
    pub fn add_attachment(&mut self, name: &str, mime: Option<&str>, data: &[u8]) -> Result<AttachmentMeta> {
        insert(&self.conn, name, mime, data)
    }

    pub fn attachment_meta(&self, id: &str) -> Result<AttachmentMeta> {
        self.conn
            .query_row("SELECT id, name, mime, size, created FROM attachments WHERE id = ?1", [id], |r| {
                Ok(AttachmentMeta {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    mime: r.get(2)?,
                    size: r.get(3)?,
                    created: r.get(4)?,
                })
            })
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("attachment {id}")))
    }

    pub fn attachment(&self, id: &str) -> Result<(AttachmentMeta, Vec<u8>)> {
        let meta = self.attachment_meta(id)?;
        let data = self.conn.query_row("SELECT data FROM attachments WHERE id = ?1", [id], |r| r.get(0))?;
        Ok((meta, data))
    }

    pub fn attachments(&self) -> Result<Vec<AttachmentMeta>> {
        let mut st = self.conn.prepare("SELECT id, name, mime, size, created FROM attachments ORDER BY created")?;
        let rows = st.query_map([], |r| {
            Ok(AttachmentMeta { id: r.get(0)?, name: r.get(1)?, mime: r.get(2)?, size: r.get(3)?, created: r.get(4)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Deletes attachments that no note references any more.
    pub fn prune_attachments(&mut self) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM attachments WHERE NOT EXISTS (
                 SELECT 1 FROM notes WHERE instr(notes.body, 'attachment:' || attachments.id) > 0)",
            [],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_types_are_strict() {
        assert!(
            is_valid_mime("image/png") && is_valid_mime("image/svg+xml") && is_valid_mime("application/vnd.ms-excel")
        );
        for bad in ["", "image", "/png", "image/", "text/html; charset=utf-8", "image/png\r\nX: y", "a\"b/c", "a b/c"] {
            assert!(!is_valid_mime(bad), "{bad:?}");
        }
    }

    #[test]
    fn ids_refs_names() {
        let id = "0f8fad5b-d9cb-469f-a165-70867728950e";
        assert!(is_valid_id(id));
        assert!(!is_valid_id("0F8FAD5B-D9CB-469F-A165-70867728950E"));
        assert!(!is_valid_id("not-an-id"));
        let body = format!("a ![x](attachment:{id}) b attachment:nope [f](attachment:{id})");
        let refs = find_refs(&body);
        assert_eq!(refs.len(), 2);
        assert_eq!(&body[refs[0].0.clone()], format!("attachment:{id}"));
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_name("C:\\x\\a:b?.png"), "a-b-.png");
        assert_eq!(sanitize_name(".."), "file");
        assert_eq!(mime_for_name("Photo.JPG"), "image/jpeg");
    }
}

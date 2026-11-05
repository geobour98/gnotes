//! Export the vault to a plain folder of Markdown files, and import such a
//! folder (an Obsidian vault, a Joplin "Markdown + Front Matter" export, or
//! any directory of .md files).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use regex::{Captures, Regex};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::attachments::{self, AttachmentMeta};
use crate::error::{Error, Result};
use crate::paths;
use crate::vault::{Vault, now_ms};

/// Folder name for attachments in an export.
pub const EXPORT_ATTACHMENTS_DIR: &str = "attachments";
const MAX_IMPORT_FILES: usize = 50_000;

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub dir: String,
    pub notes: usize,
    pub attachments: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkReport {
    /// Notes whose links were rewritten.
    pub notes: usize,
    /// Files stored as new attachments.
    pub attachments: usize,
    /// Links rewritten to attachments.
    pub links: usize,
    /// Links that still point to files that weren't found.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    /// Vault folder everything was imported into.
    pub folder: String,
    pub notes: usize,
    pub attachments: usize,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------- helpers

/// Percent-encodes characters that would break a Markdown link destination.
fn encode_link_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() || " ()<>%#?[]^`{}|\"\\".contains(c) {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
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

/// Turns any file/folder name into a valid vault name.
fn sanitize_segment(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '-' } else { c })
        .collect();
    let s = s.trim().trim_end_matches('.').trim().to_string();
    let s: String = s.chars().take(200).collect();
    if s.is_empty() || s == "." || s == ".." { "Untitled".to_string() } else { s }
}

/// `name`, `name 2`, `name 3`, ... whichever `taken` doesn't contain.
fn unique(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    (2..).map(|i| format!("{name} {i}")).find(|n| !taken(n)).unwrap()
}

/// Byte ranges of fenced code blocks, where links must not be rewritten.
fn fenced_ranges(body: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut open: Option<(usize, String)> = None;
    let mut pos = 0;
    for line in body.split_inclusive('\n') {
        let t = line.trim_start();
        let fence: String = t.chars().take_while(|&c| c == '`' || c == '~').collect();
        let is_fence = fence.len() >= 3 && fence.chars().all(|c| c == fence.chars().next().unwrap());
        match &open {
            None if is_fence => open = Some((pos, fence)),
            Some((start, f)) if is_fence && fence.starts_with(f.as_str()) && t.trim_end() == fence => {
                out.push(*start..pos + line.len());
                open = None;
            }
            _ => {}
        }
        pos += line.len();
    }
    if let Some((start, _)) = open {
        out.push(start..body.len());
    }
    out
}

// ----------------------------------------------------------------- export

impl Vault {
    /// Writes every note as `<folder>/<title>.md` under `dest` (which must not
    /// exist yet) and attachments under `dest/attachments/`, rewriting
    /// `attachment:<id>` references to relative paths. The output is NOT
    /// encrypted.
    pub fn export_to_dir(&self, dest: &Path) -> Result<ExportReport> {
        if dest.exists() {
            return Err(Error::AlreadyExists(dest.display().to_string()));
        }
        fs::create_dir_all(dest)?;
        let tree = self.tree()?;
        for folder in &tree.folders {
            fs::create_dir_all(dest.join(folder))?;
        }

        let mut written: HashMap<String, String> = HashMap::new(); // id -> file name
        let mut used_names: HashSet<String> = HashSet::new();
        for meta in &tree.notes {
            let note = self.get_note(&meta.id)?;
            let depth = if note.folder.is_empty() { 0 } else { note.folder.split('/').count() };
            let body = self.export_refs(&note.body, &"../".repeat(depth), dest, &mut written, &mut used_names)?;
            let dir = if note.folder.is_empty() { dest.to_path_buf() } else { dest.join(&note.folder) };
            fs::write(dir.join(format!("{}.md", note.title)), body)?;
        }
        Ok(ExportReport { dir: dest.display().to_string(), notes: tree.notes.len(), attachments: written.len() })
    }

    /// Rewrites the `attachment:<id>` references of `body` to
    /// `<prefix>attachments/<file>`, writing each attachment once into
    /// `dest/attachments`. Dangling references are left as they are.
    fn export_refs(
        &self,
        body: &str,
        prefix: &str,
        dest: &Path,
        written: &mut HashMap<String, String>,
        used_names: &mut HashSet<String>,
    ) -> Result<String> {
        let mut out = String::with_capacity(body.len());
        let mut last = 0;
        for (range, id) in attachments::find_refs(body) {
            let file = match written.get(id) {
                Some(f) => f.clone(),
                None => match self.attachment(id) {
                    Ok((att, data)) => {
                        let f = unique_file_name(&att.name, used_names);
                        let dir = dest.join(EXPORT_ATTACHMENTS_DIR);
                        fs::create_dir_all(&dir)?;
                        fs::write(dir.join(&f), data)?;
                        used_names.insert(f.to_lowercase());
                        written.insert(id.to_string(), f.clone());
                        f
                    }
                    Err(Error::NotFound(_)) => continue,
                    Err(e) => return Err(e),
                },
            };
            out.push_str(&body[last..range.start]);
            out.push_str(prefix);
            out.push_str(EXPORT_ATTACHMENTS_DIR);
            out.push('/');
            out.push_str(&encode_link_path(&file));
            last = range.end;
        }
        out.push_str(&body[last..]);
        Ok(out)
    }

    /// Exports one note into `parent`: `<title>.md`, or, when it has
    /// attachments, a `<title>/` folder with `<title>.md` and `attachments/`.
    /// Existing files are never overwritten (" (2)" is appended instead).
    pub fn export_note_markdown(&self, id: &str, parent: &Path) -> Result<NoteExportReport> {
        let note = self.get_note(id)?;
        let has_files = attachments::find_refs(&note.body).iter().any(|(_, id)| self.attachment_meta(id).is_ok());
        if !has_files {
            let path = unique_path(parent, &note.title, ".md");
            fs::write(&path, &note.body)?;
            return Ok(NoteExportReport { path: path.display().to_string(), attachments: 0 });
        }
        let dir = unique_path(parent, &note.title, "");
        fs::create_dir_all(&dir)?;
        let (mut written, mut used) = (HashMap::new(), HashSet::new());
        let body = self.export_refs(&note.body, "", &dir, &mut written, &mut used)?;
        let path = dir.join(format!("{}.md", note.title));
        fs::write(&path, body)?;
        Ok(NoteExportReport { path: path.display().to_string(), attachments: written.len() })
    }

    /// Writes a rendered note (a complete HTML document made by the UI) to
    /// `path`, embedding every `src="attachment:<id>"` image as a data: URL
    /// so the file is self-contained. Returns the number of embedded images.
    pub fn export_note_html(&self, html: &str, path: &Path) -> Result<usize> {
        static IMG_SRC: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"src="attachment:([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})""#).unwrap()
        });
        let mut embedded = 0;
        let mut err = None;
        let out = IMG_SRC.replace_all(html, |c: &Captures| match self.attachment(&c[1]) {
            Ok((meta, data)) if meta.mime.starts_with("image/") => {
                embedded += 1;
                format!(r#"src="data:{};base64,{}""#, meta.mime, BASE64.encode(data))
            }
            Ok(_) | Err(Error::NotFound(_)) => r#"src="""#.to_string(),
            Err(e) => {
                err.get_or_insert(e);
                String::new()
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
        fs::write(path, out.as_bytes())?;
        Ok(embedded)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteExportReport {
    /// The written .md file.
    pub path: String,
    pub attachments: usize,
}

/// `parent/<stem><ext>`, or `parent/<stem> (2)<ext>`, ... if that exists.
fn unique_path(parent: &Path, stem: &str, ext: &str) -> PathBuf {
    (1..)
        .map(|i| if i == 1 { parent.join(format!("{stem}{ext}")) } else { parent.join(format!("{stem} ({i}){ext}")) })
        .find(|p| !p.exists())
        .unwrap()
}

/// Case-insensitively unique file name: `a.png`, `a (2).png`, ...
fn unique_file_name(name: &str, used: &HashSet<String>) -> String {
    if !used.contains(&name.to_lowercase()) {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    (2..).map(|i| format!("{stem} ({i}){ext}")).find(|n| !used.contains(&n.to_lowercase())).unwrap()
}

// ----------------------------------------------------------------- import

static MD_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(!?)\[((?:[^\[\]\n]|\[[^\[\]\n]*\])*)\]\(\s*(<[^>\n]*>|[^\s()]*(?:\([^\s()]*\)[^\s()]*)*)(\s+(?:"[^"\n]*"|'[^'\n]*'))?\s*\)"#,
    )
    .unwrap()
});
static WIKI_EMBED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[\[([^\]\|\n]+?)(?:\|([^\]\n]*))?\]\]").unwrap());
static HTML_IMG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<img\b[^>]*?\bsrc\s*=\s*)("[^"]*"|'[^']*')"#).unwrap());
static HAS_SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());

struct Source {
    root: PathBuf,
    notes: Vec<PathBuf>,
    /// Lower-cased file name -> non-Markdown files (for `![[name.png]]`).
    by_name: HashMap<String, Vec<PathBuf>>,
}

fn is_markdown(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn scan(root: &Path) -> Result<Source> {
    let mut src = Source { root: root.to_path_buf(), notes: Vec::new(), by_name: HashMap::new() };
    let mut stack = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            // Skip hidden things: .obsidian, .git, .trash, ...
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            // file_type() doesn't follow symlinks: they are skipped, so the
            // import can't loop or escape the chosen folder.
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                count += 1;
                if count > MAX_IMPORT_FILES {
                    return Err(Error::TooLarge(format!("more than {MAX_IMPORT_FILES} files")));
                }
                if is_markdown(&path) {
                    src.notes.push(path);
                } else {
                    src.by_name.entry(name.to_string_lossy().to_lowercase()).or_default().push(path);
                }
            }
        }
    }
    src.notes.sort();
    Ok(src)
}

struct Importer<'a> {
    conn: &'a Connection,
    src: &'a Source,
    imported: HashMap<PathBuf, AttachmentMeta>,
    warnings: Vec<String>,
    /// Links rewritten to attachment: references.
    links: usize,
}

impl Importer<'_> {
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.src.root).unwrap_or(p).display().to_string()
    }

    /// Resolves a link target relative to the note, then the vault root.
    /// Only regular files inside the imported folder are accepted.
    fn resolve(&self, note_dir: &Path, target: &str) -> Option<PathBuf> {
        let target = target.trim_start_matches("./");
        [note_dir.join(target), self.src.root.join(target.trim_start_matches('/'))]
            .into_iter()
            .filter_map(|p| p.canonicalize().ok())
            .find(|p| p.is_file() && p.starts_with(&self.src.root))
    }

    /// Obsidian resolves `![[name.png]]` by file name anywhere in the vault.
    /// Finds the file a link points to: relative to the note (when the note
    /// has a location on disk), otherwise by file name anywhere in the folder,
    /// like Obsidian. Among same-named files, the one whose trailing path
    /// components best match the link wins.
    fn locate(&self, note_dir: Option<&Path>, target: &str) -> Option<PathBuf> {
        if let Some(p) = note_dir.and_then(|d| self.resolve(d, target)) {
            return Some(p);
        }
        let lower_parts = |p: &Path| -> Vec<String> {
            p.components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
                    _ => None,
                })
                .collect()
        };
        let want = lower_parts(Path::new(target));
        let candidates = self.src.by_name.get(want.last()?)?;
        candidates
            .iter()
            .max_by_key(|p| {
                let have = lower_parts(p.strip_prefix(&self.src.root).unwrap_or(p));
                want.iter().rev().zip(have.iter().rev()).take_while(|(a, b)| a == b).count()
            })
            .and_then(|p| p.canonicalize().ok())
    }

    fn attach(&mut self, path: &Path) -> Option<AttachmentMeta> {
        if let Some(m) = self.imported.get(path) {
            return Some(m.clone());
        }
        let data = match fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                self.warnings.push(format!("{}: {e}", self.rel(path)));
                return None;
            }
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match attachments::insert(self.conn, &name, None, &data) {
            Ok(m) => {
                self.imported.insert(path.to_path_buf(), m.clone());
                Some(m)
            }
            Err(e) => {
                self.warnings.push(format!("{}: {e}", self.rel(path)));
                None
            }
        }
    }

    /// Rewrites links to local files into `attachment:<id>` references.
    /// `note_label` names the note in warnings; `note_dir` is its folder on
    /// disk when importing (None for notes already in the vault).
    fn rewrite(&mut self, note_label: &str, note_dir: Option<&Path>, body: &str) -> String {
        let note_rel = note_label;
        let code = fenced_ranges(body);
        let in_code = |pos: usize| code.iter().any(|r| r.contains(&pos));

        // 1. Obsidian embeds: ![[image.png]] / ![[file.pdf|alias]]
        let body = WIKI_EMBED
            .replace_all(body, |c: &Captures| {
                let whole = c[0].to_string();
                let target = c[1].split('#').next().unwrap_or("").trim().to_string();
                let has_ext = Path::new(&target).extension().is_some();
                if in_code(c.get(0).unwrap().start()) || !has_ext || is_markdown(Path::new(&target)) {
                    return whole; // note embeds stay as they are
                }
                let Some(path) = self.locate(note_dir, &target) else {
                    self.warnings.push(format!("{note_rel}: missing {target}"));
                    return whole;
                };
                let Some(m) = self.attach(&path) else { return whole };
                let alias = c.get(2).map(|a| a.as_str().trim()).filter(|a| !a.is_empty() && !a.chars().all(|ch| ch.is_ascii_digit() || ch == 'x'));
                let label = alias.unwrap_or(&m.name).replace(['[', ']'], "");
                self.links += 1;
                if m.mime.starts_with("image/") {
                    format!("![{label}](attachment:{})", m.id)
                } else {
                    format!("[📎 {label}](attachment:{})", m.id)
                }
            })
            .into_owned();

        // 2. Markdown links and images to local files.
        let code = fenced_ranges(&body);
        let in_code = |pos: usize| code.iter().any(|r| r.contains(&pos));
        let body = MD_LINK
            .replace_all(&body, |c: &Captures| {
                let whole = c[0].to_string();
                let raw = c[3].trim_start_matches('<').trim_end_matches('>');
                if in_code(c.get(0).unwrap().start()) || raw.is_empty() || raw.starts_with('#') || HAS_SCHEME.is_match(raw) {
                    return whole;
                }
                let target = percent_decode(raw.split(['#', '?']).next().unwrap_or(""));
                if is_markdown(Path::new(&target)) {
                    return whole; // links between notes: kept for later wikilink support
                }
                let Some(path) = self.locate(note_dir, &target) else {
                    if !c[1].is_empty() {
                        self.warnings.push(format!("{note_rel}: missing {target}"));
                    }
                    return whole;
                };
                let Some(m) = self.attach(&path) else { return whole };
                self.links += 1;
                format!("{}[{}](attachment:{}{})", &c[1], &c[2], m.id, c.get(4).map_or("", |t| t.as_str()))
            })
            .into_owned();

        // 3. Raw HTML <img src="local.png">
        let code = fenced_ranges(&body);
        let in_code = |pos: usize| code.iter().any(|r| r.contains(&pos));
        HTML_IMG
            .replace_all(&body, |c: &Captures| {
                let whole = c[0].to_string();
                let raw = &c[2][1..c[2].len() - 1];
                if in_code(c.get(0).unwrap().start()) || HAS_SCHEME.is_match(raw) || raw.is_empty() {
                    return whole;
                }
                let target = percent_decode(raw.split(['#', '?']).next().unwrap_or(""));
                match self.locate(note_dir, &target).and_then(|p| self.attach(&p)) {
                    Some(m) => {
                        self.links += 1;
                        format!("{}\"attachment:{}\"", &c[1], m.id)
                    }
                    None => whole,
                }
            })
            .into_owned()
    }
}

impl Vault {
    /// Imports a folder of Markdown files (and the files they reference) into
    /// vault folder `into` ("" = top level, created if missing). With
    /// `new_folder`, everything goes into a new subfolder named after the
    /// source; otherwise it is merged into `into` (clashing titles get a
    /// number). Everything happens in one transaction: on a fatal error
    /// nothing is imported.
    pub fn import_dir(&mut self, src_dir: &Path, into: &str, new_folder: bool) -> Result<ImportReport> {
        let into = into.trim().trim_matches('/');
        paths::validate_folder(into)?;
        let root = src_dir.canonicalize().map_err(|_| Error::NotFound(src_dir.display().to_string()))?;
        if !root.is_dir() {
            return Err(Error::NotFound(format!("{} is not a folder", root.display())));
        }
        let src = scan(&root)?;
        if src.notes.is_empty() {
            return Err(Error::NotFound("no Markdown (.md) files in that folder".into()));
        }

        let tree = self.tree()?;
        let existing: HashSet<String> = tree.folders.into_iter().collect();
        let top = if new_folder {
            let base = sanitize_segment(&root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            unique(&paths::join(into, &base), |n| existing.contains(n))
        } else {
            into.to_string()
        };
        // Titles already in the vault, so merged notes never collide.
        let mut titles: HashSet<(String, String)> = tree.notes.into_iter().map(|n| (n.folder, n.title)).collect();
        let mut imported_notes = 0;

        let tx = self.conn.transaction()?;
        let mut imp = Importer { conn: &tx, src: &src, imported: HashMap::new(), warnings: Vec::new(), links: 0 };
        let mut folders: HashSet<String> = HashSet::new();

        for note in &src.notes {
            let rel_dir = note.parent().and_then(|d| d.strip_prefix(&root).ok()).unwrap_or(Path::new(""));
            let mut folder = top.clone();
            for seg in rel_dir.components() {
                folder = paths::join(&folder, &sanitize_segment(&seg.as_os_str().to_string_lossy()));
            }
            for f in paths::ancestors(&folder) {
                if folders.insert(f.clone()) {
                    tx.execute("INSERT OR IGNORE INTO folders(path) VALUES (?1)", [&f])?;
                }
            }
            let stem = note.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let title = unique(&sanitize_segment(&stem), |t| titles.contains(&(folder.clone(), t.to_string())));
            titles.insert((folder.clone(), title.clone()));

            let raw = match fs::read(note) {
                Ok(b) => b,
                Err(e) => {
                    imp.warnings.push(format!("{}: {e}", imp.rel(note)));
                    continue;
                }
            };
            let label = imp.rel(note);
            let body = imp.rewrite(&label, note.parent(), &String::from_utf8_lossy(&raw));
            let modified = fs::metadata(note)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or_else(now_ms, |d| d.as_millis() as i64);
            tx.execute(
                "INSERT INTO notes(id, folder, title, body, created, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![uuid::Uuid::new_v4().to_string(), folder, title, body, modified],
            )?;
            imported_notes += 1;
        }
        let report = ImportReport {
            folder: top,
            notes: imported_notes,
            attachments: imp.imported.len(),
            warnings: imp.warnings,
        };
        tx.commit()?;
        Ok(report)
    }
}

impl Vault {
    /// Repairs links in existing notes that point to local files (e.g. a
    /// shared `img/` folder that wasn't imported): every referenced file
    /// found in `dir` (searched recursively, matched by file name) is stored
    /// once and all links to it become `attachment:` references.
    /// Unreferenced files are ignored. One transaction: all or nothing.
    pub fn link_files_from_dir(&mut self, dir: &Path) -> Result<LinkReport> {
        let root = dir.canonicalize().map_err(|_| Error::NotFound(dir.display().to_string()))?;
        if !root.is_dir() {
            return Err(Error::NotFound(format!("{} is not a folder", root.display())));
        }
        let src = scan(&root)?;
        let notes: Vec<(String, String, String)> = {
            let mut st = self.conn.prepare("SELECT id, folder, title, body FROM notes ORDER BY folder, title")?;
            st.query_map([], |r| {
                let (id, folder, title, body): (String, String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
                Ok((id, paths::join(&folder, &title), body))
            })?
            .collect::<rusqlite::Result<_>>()?
        };

        let tx = self.conn.transaction()?;
        let mut imp = Importer { conn: &tx, src: &src, imported: HashMap::new(), warnings: Vec::new(), links: 0 };
        let mut changed = 0;
        for (id, label, body) in &notes {
            let new_body = imp.rewrite(label, None, body);
            if new_body != *body {
                // `updated` is left alone: this is a repair, not an edit.
                tx.execute("UPDATE notes SET body = ?2 WHERE id = ?1", params![id, new_body])?;
                changed += 1;
            }
        }
        let report = LinkReport { notes: changed, attachments: imp.imported.len(), links: imp.links, warnings: imp.warnings };
        tx.commit()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(encode_link_path("my pic (1).png"), "my%20pic%20%281%29.png");
        assert_eq!(percent_decode("my%20pic%20%281%29.png"), "my pic (1).png");
        assert_eq!(sanitize_segment("a:b/c?"), "a-b-c-");
        assert_eq!(sanitize_segment(" .. "), "Untitled");
        assert_eq!(unique("x", |n| n == "x" || n == "x 2"), "x 3");
        let body = "a\n```\n![x](y.png)\n```\nb ![z](w.png)\n";
        let r = fenced_ranges(body);
        assert_eq!(r.len(), 1);
        assert!(r[0].contains(&body.find("![x]").unwrap()));
        assert!(!r[0].contains(&body.find("![z]").unwrap()));
    }
}

//! Folder paths and note titles inside the vault.
//!
//! Folders are virtual: `""` is the root, `"projects/thesis"` is nested.
//! Names follow filesystem-safe rules so the vault can always be exported
//! to a real directory of `.md` files.

use crate::error::{Error, Result};

const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Validates a single path segment (folder name or note title).
pub fn validate_name(name: &str) -> Result<()> {
    let bad = |why: &str| Err(Error::InvalidName(format!("{name:?}: {why}")));
    if name.trim().is_empty() {
        return bad("must not be empty");
    }
    if name != name.trim() {
        return bad("must not start or end with whitespace");
    }
    if name == "." || name == ".." {
        return bad("reserved name");
    }
    if name.ends_with('.') {
        return bad("must not end with a dot");
    }
    if name.chars().count() > 200 {
        return bad("too long");
    }
    if let Some(c) = name.chars().find(|c| c.is_control() || FORBIDDEN.contains(c)) {
        return bad(&format!("must not contain {c:?}"));
    }
    Ok(())
}

/// Validates a folder path; `""` (root) is valid.
pub fn validate_folder(path: &str) -> Result<()> {
    if path.is_empty() {
        return Ok(());
    }
    path.split('/').try_for_each(validate_name)
}

/// All ancestors of a folder, excluding root, ending with the folder itself.
/// `"a/b/c"` -> `["a", "a/b", "a/b/c"]`.
pub fn ancestors(path: &str) -> Vec<String> {
    if path.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for seg in path.split('/') {
        if !cur.is_empty() {
            cur.push('/');
        }
        cur.push_str(seg);
        out.push(cur.clone());
    }
    out
}

pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

pub fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// True if `path` equals `folder` or lies inside it.
pub fn is_within(path: &str, folder: &str) -> bool {
    path == folder || path.starts_with(&format!("{folder}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(validate_name("Meeting notes 2026").is_ok());
        assert!(validate_name("Ελληνικά").is_ok());
        for bad in ["", " a", "a ", "..", "a/b", "a:b", "end.", "tab\t"] {
            assert!(validate_name(bad).is_err(), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn folders() {
        assert!(validate_folder("").is_ok());
        assert!(validate_folder("a/b").is_ok());
        assert!(validate_folder("/a").is_err());
        assert!(validate_folder("a//b").is_err());
        assert!(validate_folder("a/../b").is_err());
        assert_eq!(ancestors("a/b/c"), ["a", "a/b", "a/b/c"]);
        assert_eq!(parent("a/b/c"), "a/b");
        assert_eq!(parent("a"), "");
        assert!(is_within("a/b", "a"));
        assert!(!is_within("ab", "a"));
    }
}

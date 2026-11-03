//! Tags, following Obsidian's rules:
//!
//! - inline `#tag` preceded by the start of a line or whitespace; letters,
//!   digits, `_`, `-` and `/` (for nesting: `#ai/llm`); not only digits,
//!   so `#123` is not a tag; `# Heading` is not a tag;
//! - `tags:` (or `tag:`) in YAML front matter, as `[a, b]`, `a, b` or a
//!   `- a` list;
//! - nothing inside fenced code blocks or `inline code`.
//!
//! Tags are case-insensitive and returned in lowercase. Like search, they are
//! computed from the unlocked vault on demand; nothing is stored separately.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::error::Result;
use crate::vault::Vault;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TagCount {
    pub tag: String,
    /// Notes carrying this tag or one nested under it.
    pub count: usize,
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Normalizes a tag name (without `#`): lowercase, empty `/` segments
/// removed. `None` when it isn't a valid tag.
pub fn normalize(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_start_matches('#');
    if raw.is_empty() || !raw.chars().all(is_tag_char) {
        return None;
    }
    let tag = raw.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("/").to_lowercase();
    if tag.is_empty() || tag.chars().all(|c| c.is_ascii_digit() || c == '/') {
        return None;
    }
    Some(tag)
}

/// `ai/llm/rag` → `ai`, `ai/llm`, `ai/llm/rag`.
pub fn with_ancestors(tag: &str) -> impl Iterator<Item = &str> {
    tag.match_indices('/').map(move |(i, _)| &tag[..i]).chain(std::iter::once(tag))
}

/// Inline tags on one line of text, with `inline code` skipped.
fn line_tags(line: &str, out: &mut BTreeSet<String>) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '`' => {
                // Skip a code span: a run of n backticks up to the next run of n.
                let n = chars[i..].iter().take_while(|&&c| c == '`').count();
                let mut j = i + n;
                let mut closed = None;
                while j < chars.len() {
                    if chars[j] == '`' {
                        let m = chars[j..].iter().take_while(|&&c| c == '`').count();
                        if m == n {
                            closed = Some(j + m);
                            break;
                        }
                        j += m;
                    } else {
                        j += 1;
                    }
                }
                i = closed.unwrap_or(i + n);
            }
            '#' if i == 0 || chars[i - 1].is_whitespace() => {
                let end = i + 1 + chars[i + 1..].iter().take_while(|&&c| is_tag_char(c)).count();
                let raw: String = chars[i + 1..end].iter().collect();
                if let Some(tag) = normalize(&raw) {
                    out.insert(tag);
                }
                i = end.max(i + 1);
            }
            _ => i += 1,
        }
    }
}

/// Front matter (`---` … `---`/`...` at the very top): returns the lines
/// inside it and the index of the first body line after it.
fn front_matter(lines: &[&str]) -> Option<(usize, usize)> {
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return None;
    }
    let close = lines[1..].iter().position(|l| matches!(l.trim_end(), "---" | "..."))?;
    Some((1, close + 1))
}

fn front_matter_tags(lines: &[&str], out: &mut BTreeSet<String>) {
    let mut in_list = false;
    for line in lines {
        let trimmed = line.trim();
        if in_list {
            if let Some(item) = trimmed.strip_prefix("- ").or(if trimmed == "-" { Some("") } else { None }) {
                add_value(item, out);
                continue;
            }
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            in_list = false;
        }
        // Only top-level keys (not indented).
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        if !matches!(key.trim().to_lowercase().as_str(), "tags" | "tag") {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            in_list = true;
        } else {
            let value = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')).unwrap_or(value);
            for item in value.split([',', ' ']) {
                add_value(item, out);
            }
        }
    }
}

fn add_value(item: &str, out: &mut BTreeSet<String>) {
    let item = item.trim().trim_matches(|c| c == '"' || c == '\'');
    if let Some(tag) = normalize(item) {
        out.insert(tag);
    }
}

/// All tags of a note body (front matter and inline), lowercase, sorted.
pub fn extract(body: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = body.lines().collect();
    let mut out = BTreeSet::new();
    let mut start = 0;
    if let Some((from, to)) = front_matter(&lines) {
        front_matter_tags(&lines[from..to], &mut out);
        start = to + 1;
    }
    let mut fence: Option<(char, usize)> = None;
    for line in lines.iter().skip(start) {
        let t = line.trim_start();
        let indent = line.len() - t.len();
        let marker = t.chars().next().filter(|c| matches!(c, '`' | '~'));
        let run = marker.map(|m| t.chars().take_while(|&c| c == m).count()).unwrap_or(0);
        match fence {
            Some((m, n)) => {
                if marker == Some(m) && run >= n && t[run..].trim().is_empty() {
                    fence = None;
                }
            }
            None if indent <= 3 && run >= 3 => fence = marker.map(|m| (m, run)),
            None => line_tags(line, &mut out),
        }
    }
    out
}

/// Does a note carrying `tags` match the filter `tag` (itself or nested)?
pub fn has_tag(tags: &BTreeSet<String>, tag: &str) -> bool {
    tags.iter().any(|t| t == tag || (t.starts_with(tag) && t[tag.len()..].starts_with('/')))
}

impl Vault {
    /// Every tag in the vault (including parents of nested tags), sorted,
    /// with the number of notes under each.
    pub fn tags(&self) -> Result<Vec<TagCount>> {
        let mut st = self.conn.prepare("SELECT body FROM notes")?;
        let mut rows = st.query([])?;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        while let Some(r) = rows.next()? {
            let body: String = r.get(0)?;
            let mut seen = BTreeSet::new();
            for tag in extract(&body) {
                for t in with_ancestors(&tag) {
                    seen.insert(t.to_string());
                }
            }
            for t in seen {
                *counts.entry(t).or_default() += 1;
            }
        }
        Ok(counts.into_iter().map(|(tag, count)| TagCount { tag, count }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(body: &str) -> Vec<String> {
        extract(body).into_iter().collect()
    }

    #[test]
    fn inline_tags() {
        assert_eq!(tags("#ai notes about #Work and #ai/LLM."), ["ai", "ai/llm", "work"]);
        assert_eq!(tags("start #a-b_c/d-e end"), ["a-b_c/d-e"]);
        assert_eq!(tags("#ünïcode #日本"), ["ünïcode", "日本"]);
    }

    #[test]
    fn things_that_are_not_tags() {
        assert!(tags("# Heading\n## Sub").is_empty());
        assert!(tags("issue #123 and #2024/10").is_empty());
        assert!(tags("url http://x.com/page#frag and a#b and &#123;").is_empty());
        assert!(tags("##double").is_empty());
        assert!(tags("`#code` and ``x #code2 x``").is_empty());
        assert!(tags("```\n#incode\n```\n~~~~\n#also\n~~~~").is_empty());
        assert_eq!(tags("```\n#in\n```\n#after"), ["after"]);
        assert_eq!(tags("`unclosed #tag"), ["tag"]);
    }

    #[test]
    fn front_matter() {
        assert_eq!(tags("---\ntags: [AI, \"work\", '#x']\n---\nbody #b"), ["ai", "b", "work", "x"]);
        assert_eq!(tags("---\ntitle: t\ntags:\n  - one\n  - two/three\nother: z\n---\n"), ["one", "two/three"]);
        assert_eq!(tags("---\ntag: a, b c\n---"), ["a", "b", "c"]);
        // Not at the very top: an ordinary horizontal rule, no front matter.
        assert!(tags("text\n---\ntags: [a]\n---").is_empty());
        // Inside front matter, #comments are not inline tags.
        assert!(tags("---\n# comment #nope\nkey: v\n---").is_empty());
    }

    #[test]
    fn normalizing_and_nesting() {
        assert_eq!(normalize("#AI//LLM/"), Some("ai/llm".into()));
        assert_eq!(normalize("12"), None);
        assert_eq!(normalize("has space"), None);
        assert_eq!(with_ancestors("a/b/c").collect::<Vec<_>>(), ["a", "a/b", "a/b/c"]);
        let set: BTreeSet<String> = ["ai/llm".to_string()].into();
        assert!(has_tag(&set, "ai") && has_tag(&set, "ai/llm"));
        assert!(!has_tag(&set, "a") && !has_tag(&set, "ai/l"));
    }
}

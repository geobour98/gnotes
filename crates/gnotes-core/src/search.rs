//! Full-text search over the unlocked vault.
//!
//! Notes are scanned in memory, straight from the encrypted database: there
//! is no separate index, so nothing about note contents is ever written
//! anywhere else. A scan of a few thousand notes takes milliseconds.

use serde::Serialize;

use crate::error::Result;
use crate::vault::Vault;

/// At most this many notes are returned.
pub const MAX_HITS: usize = 200;
/// At most this many matching lines are returned per note.
pub const MAX_LINES: usize = 5;
/// Snippets longer than this (in characters) are cut around the match.
const SNIPPET_CHARS: usize = 140;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LineMatch {
    /// 0-based line number in the note body.
    pub line: usize,
    /// The line (or a window of it around the first match), trimmed.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchHit {
    pub id: String,
    pub folder: String,
    pub title: String,
    pub updated: i64,
    /// Some search term matched the title or folder path.
    pub title_match: bool,
    /// Matching lines, in order, at most [`MAX_LINES`].
    pub lines: Vec<LineMatch>,
    /// Number of lines that matched (may exceed `lines.len()`).
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Number of matching notes, including those past [`MAX_HITS`].
    pub total: usize,
    /// The parsed, lowercase terms, so the UI can highlight them.
    pub terms: Vec<String>,
}

/// Splits a query into lowercase terms. `"quoted phrases"` stay together.
pub fn parse_query(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in query.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                push_term(&mut terms, &mut cur);
            }
            c if c.is_whitespace() && !quoted => push_term(&mut terms, &mut cur),
            c => cur.push(c),
        }
    }
    push_term(&mut terms, &mut cur);
    terms.dedup();
    terms
}

fn push_term(terms: &mut Vec<String>, cur: &mut String) {
    let t = cur.trim().to_lowercase();
    if !t.is_empty() && !terms.contains(&t) {
        terms.push(t);
    }
    cur.clear();
}

/// Cuts `line` to about [`SNIPPET_CHARS`] characters around the first match.
fn snippet(line: &str, lower: &str, terms: &[String]) -> String {
    let line = line.trim();
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= SNIPPET_CHARS {
        return line.to_string();
    }
    // Character position of the first match (lowercasing rarely changes
    // lengths, so this is a close approximation in the worst case).
    let lower = lower.trim();
    let first = terms
        .iter()
        .filter_map(|t| lower.find(t.as_str()))
        .min()
        .map(|b| lower[..b].chars().count())
        .unwrap_or(0)
        .min(chars.len());
    let start = first.saturating_sub(SNIPPET_CHARS / 3);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let start = end.saturating_sub(SNIPPET_CHARS);
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

impl Vault {
    /// Finds notes containing every term of `query` (case-insensitive) in
    /// their title, folder path or body. Notes whose title matches come
    /// first, then those with more matching lines, then the most recent.
    pub fn search(&self, query: &str) -> Result<SearchResults> {
        let terms = parse_query(query);
        if terms.is_empty() {
            return Ok(SearchResults { hits: Vec::new(), total: 0, terms });
        }
        let mut st = self.conn.prepare("SELECT id, folder, title, body, updated FROM notes")?;
        let mut rows = st.query([])?;
        let mut hits = Vec::new();
        while let Some(r) = rows.next()? {
            let folder: String = r.get(1)?;
            let title: String = r.get(2)?;
            let body: String = r.get(3)?;
            let path = format!("{folder}/{title}").to_lowercase();
            let body_lower = body.to_lowercase();
            if !terms.iter().all(|t| path.contains(t.as_str()) || body_lower.contains(t.as_str())) {
                continue;
            }
            let title_match = terms.iter().any(|t| path.contains(t.as_str()));
            let mut lines = Vec::new();
            let mut line_count = 0;
            for (i, line) in body.lines().enumerate() {
                let lower = line.to_lowercase();
                if terms.iter().any(|t| lower.contains(t.as_str())) {
                    line_count += 1;
                    if lines.len() < MAX_LINES {
                        lines.push(LineMatch { line: i, text: snippet(line, &lower, &terms) });
                    }
                }
            }
            hits.push(SearchHit {
                id: r.get(0)?,
                folder,
                title,
                updated: r.get(4)?,
                title_match,
                lines,
                line_count,
            });
        }
        hits.sort_by(|a, b| {
            b.title_match
                .cmp(&a.title_match)
                .then(b.line_count.cmp(&a.line_count))
                .then(b.updated.cmp(&a.updated))
        });
        let total = hits.len();
        hits.truncate(MAX_HITS);
        Ok(SearchResults { hits, total, terms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_terms_and_phrases() {
        assert_eq!(parse_query("  Rust  vault "), vec!["rust", "vault"]);
        assert_eq!(parse_query(r#"ai "Large Language" ai"#), vec!["ai", "large language"]);
        assert_eq!(parse_query(r#""unterminated phrase"#), vec!["unterminated phrase"]);
        assert!(parse_query("  \"\" ").is_empty());
    }

    #[test]
    fn snippets_are_cut_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(300), "b".repeat(300));
        let s = snippet(&line, &line.to_lowercase(), &["needle".into()]);
        assert!(s.contains("needle"));
        assert!(s.starts_with('…') && s.ends_with('…'));
        assert!(s.chars().count() <= SNIPPET_CHARS + 2);
        assert_eq!(snippet("  short  ", "  short  ", &["short".into()]), "short");
    }
}

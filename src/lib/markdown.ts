// Markdown -> sanitized HTML.
//
// Raw HTML in notes is allowed (e.g. <details>, <kbd>), so everything goes
// through DOMPurify. This matters: imported notes may be untrusted, and
// script running in the webview could call backend commands.
import MarkdownIt from "markdown-it";
import footnote from "markdown-it-footnote";
import mark from "markdown-it-mark";
import texmath from "markdown-it-texmath";
import katex from "katex";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import { attachmentUrl, ATTACHMENT_ID } from "./api";

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

/** GitHub-style heading slug, also used to resolve `#Heading Text` links. */
export function slugify(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .replace(/\s+/g, "-");
}

const md = new MarkdownIt({
  html: true,
  linkify: true,
  typographer: false,
  highlight(code, lang) {
    const language = lang && hljs.getLanguage(lang) ? lang : null;
    // markdown-it wraps this in <pre><code ...>, keeping our data-line attr.
    return language ? hljs.highlight(code, { language, ignoreIllegals: true }).value : escapeHtml(code);
  },
});

md.use(footnote); // [^1]
md.use(mark); // ==highlight==
md.use(texmath, {
  // $inline$ and $$block$$ math, rendered locally by KaTeX.
  engine: katex,
  delimiters: "dollars",
  katexOptions: { throwOnError: false, trust: false, strict: "ignore", output: "htmlAndMathml" },
});

// ```mermaid blocks become placeholders that the preview renders lazily.
const defaultFence = md.renderer.rules.fence!;
// A `wrap` word in the fence info (```bash wrap) soft-wraps that block;
// other Markdown tools ignore the extra word.
md.renderer.rules.fence = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  const words = token.info.trim().toLowerCase().split(/\s+/);
  if (words[0] === "mermaid") {
    return `<div class="mermaid"${self.renderAttrs(token)}>${escapeHtml(token.content)}</div>\n`;
  }
  const html = defaultFence(tokens, idx, options, env, self);
  return words.includes("wrap") ? html.replace(/^<pre>/, '<pre class="wrap">') : html;
};

/**
 * Toggles the `wrap` word on a code fence line, e.g. "```bash" <-> "```bash wrap".
 * Returns null if the line isn't an opening fence.
 */
export function toggleFenceWrap(line: string): string | null {
  const m = /^(.*?)(`{3,}|~{3,})(.*)$/.exec(line);
  if (!m || (m[2][0] === "`" && m[3].includes("`"))) return null;
  const words = m[3].trim().split(/\s+/).filter(Boolean);
  const i = words.findIndex((w) => w.toLowerCase() === "wrap");
  if (i >= 0) words.splice(i, 1);
  else words.push("wrap");
  // No language: "``` wrap", so it doesn't read like a language name.
  const info = words.join(" ");
  return m[1] + m[2] + (words[0]?.toLowerCase() === "wrap" ? " " : "") + info;
}

// GitHub-style task lists: "- [ ] todo" / "- [x] done".
md.core.ruler.after("inline", "task-lists", (state) => {
  const tokens = state.tokens;
  for (let i = 2; i < tokens.length; i++) {
    const inline = tokens[i];
    if (inline.type !== "inline" || tokens[i - 1].type !== "paragraph_open") continue;
    if (tokens[i - 2].type !== "list_item_open") continue;
    const first = inline.children?.[0];
    const m = first?.type === "text" ? /^\[([ xX])\]\s/.exec(first.content) : null;
    if (!first || !m) continue;
    first.content = first.content.slice(m[0].length);
    const box = new state.Token("html_inline", "", 0);
    box.content = `<input type="checkbox" disabled${m[1] === " " ? "" : " checked"}> `;
    inline.children!.unshift(box);
    tokens[i - 2].attrJoin("class", "task-item");
  }
});

/** Prefix for heading ids, so they can't collide with DOM properties
 * (DOMPurify strips ids like "images" or "title"). */
export const HEADING_ID_PREFIX = "h-";

/** Finds the element a `#target` link points to. */
export function findAnchor(root: ParentNode, target: string): Element | null {
  const byId = (id: string) => root.querySelector(`[id="${CSS.escape(id)}"]`);
  return byId(target) ?? byId(HEADING_ID_PREFIX + target) ?? byId(HEADING_ID_PREFIX + slugify(target));
}

// Heading ids so `#section` links (and the footnote links) can jump.
md.core.ruler.after("inline", "heading-ids", (state) => {
  const seen = new Map<string, number>();
  const tokens = state.tokens;
  for (let i = 0; i < tokens.length - 1; i++) {
    if (tokens[i].type !== "heading_open") continue;
    const base = slugify(tokens[i + 1].content) || "section";
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    tokens[i].attrSet("id", HEADING_ID_PREFIX + (n ? `${base}-${n}` : base));
  }
});

// Obsidian/GitHub callouts:  > [!warning] Optional title
const CALLOUT_GROUP: Record<string, string> = {
  note: "info",
  info: "info",
  todo: "info",
  tip: "ok",
  hint: "ok",
  important: "ok",
  success: "ok",
  check: "ok",
  done: "ok",
  warning: "warn",
  caution: "warn",
  attention: "warn",
  question: "warn",
  help: "warn",
  faq: "warn",
  danger: "bad",
  error: "bad",
  bug: "bad",
  failure: "bad",
  fail: "bad",
  missing: "bad",
  example: "neutral",
  quote: "neutral",
  cite: "neutral",
  abstract: "neutral",
  summary: "neutral",
  tldr: "neutral",
};
md.core.ruler.after("block", "callouts", (state) => {
  const tokens = state.tokens;
  for (let i = 0; i < tokens.length - 2; i++) {
    if (tokens[i].type !== "blockquote_open" || tokens[i + 1].type !== "paragraph_open") continue;
    const inline = tokens[i + 2];
    const m = /^\[!([\w-]+)\][+-]?[ \t]*([^\n]*)(?:\n|$)/.exec(inline.content);
    if (!m) continue;
    const type = m[1].toLowerCase();
    const title = m[2].trim() || type.charAt(0).toUpperCase() + type.slice(1);
    tokens[i].attrJoin("class", `callout callout-${CALLOUT_GROUP[type] ?? "info"}`);
    const head = new state.Token("html_block", "", 0);
    head.content = `<div class="callout-title">${escapeHtml(title)}</div>\n`;
    inline.content = inline.content.slice(m[0].length);
    if (inline.content.trim()) {
      tokens.splice(i + 1, 0, head);
    } else {
      tokens.splice(i + 1, 3, head); // title only: drop the empty paragraph
    }
  }
});

// Remote images are blocked (privacy: loading them tells the server you read
// the note). Show a placeholder instead of a broken image.
const defaultImage = md.renderer.rules.image!;
md.renderer.rules.image = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  const src = String(token.attrGet("src") ?? "");
  if (/^https?:/i.test(src)) {
    let host = src;
    try {
      host = new URL(src).host;
    } catch {}
    const alt = token.content || "image";
    return `<span class="remote-image" title="${escapeHtml(src)}">🖼 ${escapeHtml(alt)} <small>remote image from ${escapeHtml(host)} not loaded</small></span>`;
  }
  token.attrSet("loading", "lazy");
  return defaultImage(tokens, idx, options, env, self);
};

// #tags become clickable pills. Same rules as the Rust side (tags.rs):
// preceded by start or whitespace, letters/digits/_-/, not only digits.
// Code spans and blocks never reach inline rules, so they are excluded.
const TAG_RE = /^#([\p{L}\p{N}_\-/]+)/u;

/** "AI//LLM/" → "ai/llm"; null if it isn't a valid tag. */
export function normalizeTag(raw: string): string | null {
  const tag = raw.split("/").filter(Boolean).join("/").toLowerCase();
  return tag && !/^[\d/]+$/.test(tag) ? tag : null;
}

md.inline.ruler.after("text", "tag", (state, silent) => {
  const pos = state.pos;
  if (state.src.charCodeAt(pos) !== 0x23 /* # */) return false;
  if (pos > 0 && !/\s/.test(state.src[pos - 1])) return false;
  const m = TAG_RE.exec(state.src.slice(pos));
  const tag = m && normalizeTag(m[1]);
  if (!m || !tag) return false;
  if (!silent) {
    const token = state.push("tag", "", 0);
    token.content = m[1];
    token.info = tag;
  }
  state.pos += m[0].length;
  return true;
});
md.renderer.rules.tag = (tokens, idx) => {
  const t = tokens[idx];
  return `<span class="tag" data-tag="${escapeHtml(t.info)}">#${escapeHtml(t.content)}</span>`;
};

// Tag every block with its source line (0-based) so the editor and preview
// can scroll in sync, the same technique VS Code uses.
md.core.ruler.push("source-lines", (state) => {
  for (const token of state.tokens) {
    if (token.map && token.block && token.nesting !== -1) {
      token.attrSet("data-line", String(token.map[0]));
    }
  }
});

// ------------------------------------------------------------ sanitizing

DOMPurify.addHook("uponSanitizeElement", (node, data) => {
  // Only allow inputs that are (disabled) checkboxes from task lists.
  if (data.tagName === "input") {
    const el = node as Element;
    if (el.getAttribute("type") !== "checkbox") el.remove();
    else el.setAttribute("disabled", "");
  }
});

// KaTeX positions glyphs with inline styles; keep those (inside .katex
// only), but nothing that could load resources or overlay the app.
DOMPurify.addHook("uponSanitizeAttribute", (node, data) => {
  if (data.attrName !== "style") return;
  const inKatex = (node as Element).closest?.(".katex, .katex-display");
  if (inKatex && !/url\(|expression|fixed|absolute|@import|\\/i.test(data.attrValue)) {
    data.forceKeepAttr = true;
  } else {
    data.keepAttr = false;
  }
});

// Point media at the in-memory attachment server; mark attachment links.
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  for (const attr of ["src", "href"]) {
    const value = node.getAttribute(attr);
    if (!value?.startsWith("attachment:")) continue;
    const m = ATTACHMENT_ID.exec(value);
    if (!m) node.removeAttribute(attr);
    else if (attr === "src") {
      node.setAttribute("src", attachmentUrl(m[1]));
      node.setAttribute("data-attachment", m[1]); // for the right-click menu
    } else node.classList.add("attachment-link");
  }
});

// Link titles ([text](url "title")) go into our hover tooltip instead of the
// native one, which would otherwise pop up on top of it.
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A" && node.hasAttribute("title")) {
    node.setAttribute("data-title", node.getAttribute("title")!);
    node.removeAttribute("title");
  }
});

const PURIFY = {
  FORBID_TAGS: ["style", "form", "button", "textarea", "select", "iframe", "object", "embed"],
  FORBID_ATTR: ["srcset"],
  ALLOW_DATA_ATTR: false,
  ADD_ATTR: ["data-line", "data-tag", "loading"],
  // DOMPurify's default plus our attachment: and gnotes: schemes.
  ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto|tel|attachment|gnotes):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
};

// YAML front matter (--- ... --- at the very top) is shown as a properties
// box instead of being parsed as Markdown.
const FRONT_MATTER = /^---[ \t]*\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/;

export function render(source: string): string {
  let head = "";
  const fm = FRONT_MATTER.exec(source);
  if (fm) {
    head = `<div class="frontmatter" data-line="0"><div class="frontmatter-label">Properties</div><pre>${escapeHtml(fm[1])}</pre></div>\n`;
    // Blank lines keep the source line numbers (scroll sync) intact.
    source = "\n".repeat(fm[0].split("\n").length - 1) + source.slice(fm[0].length);
  }
  return DOMPurify.sanitize(head + md.render(source), PURIFY) as string;
}

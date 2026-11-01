// Markdown -> sanitized HTML.
//
// Raw HTML in notes is allowed (e.g. <details>, <kbd>), so everything goes
// through DOMPurify. This matters: clipped web pages are untrusted, and
// script running in the webview could call backend commands.
import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import { attachmentUrl, ATTACHMENT_ID } from "./api";

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

const md = new MarkdownIt({
  html: true,
  linkify: true,
  typographer: false,
  highlight(code, lang) {
    const language = lang && hljs.getLanguage(lang) ? lang : null;
    const html = language
      ? hljs.highlight(code, { language, ignoreIllegals: true }).value
      : escapeHtml(code);
    // markdown-it wraps this in <pre><code ...>, keeping our data-line attr.
    return html;
  },
});

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

// Tag every block with its source line (0-based) so the editor and preview
// can scroll in sync, the same technique VS Code uses.
md.core.ruler.push("source-lines", (state) => {
  for (const token of state.tokens) {
    if (token.map && token.block && token.nesting !== -1) {
      token.attrSet("data-line", String(token.map[0]));
    }
  }
});

DOMPurify.addHook("uponSanitizeElement", (node, data) => {
  // Only allow inputs that are (disabled) checkboxes from task lists.
  if (data.tagName === "input") {
    const el = node as Element;
    if (el.getAttribute("type") !== "checkbox") el.remove();
    else el.setAttribute("disabled", "");
  }
});

// Point media at the in-memory attachment server; mark attachment links.
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  for (const attr of ["src", "href"]) {
    const value = node.getAttribute(attr);
    if (!value?.startsWith("attachment:")) continue;
    const m = ATTACHMENT_ID.exec(value);
    if (!m) node.removeAttribute(attr);
    else if (attr === "src") node.setAttribute("src", attachmentUrl(m[1]));
    else node.classList.add("attachment-link");
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
  FORBID_ATTR: ["style", "srcset"],
  ALLOW_DATA_ATTR: false,
  ADD_ATTR: ["data-line", "loading"],
  // DOMPurify's default plus our attachment: and gnotes: schemes.
  ALLOWED_URI_REGEXP:
    /^(?:(?:https?|mailto|tel|attachment|gnotes):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
};

export function render(source: string): string {
  return DOMPurify.sanitize(md.render(source), PURIFY) as string;
}

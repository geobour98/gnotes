// Markdown -> sanitized HTML.
//
// Raw HTML in notes is allowed (e.g. <details>, <kbd>), so everything goes
// through DOMPurify. This matters: clipped web pages are untrusted, and
// script running in the webview could call backend commands.
import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";

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
    return `<pre class="hljs"><code>${html}</code></pre>`;
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

DOMPurify.addHook("uponSanitizeElement", (node, data) => {
  // Only allow inputs that are (disabled) checkboxes from task lists.
  if (data.tagName === "input") {
    const el = node as Element;
    if (el.getAttribute("type") !== "checkbox") el.remove();
    else el.setAttribute("disabled", "");
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
};

export function render(source: string): string {
  return DOMPurify.sanitize(md.render(source), PURIFY) as string;
}

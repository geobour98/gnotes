// Single-note export: a self-contained HTML file, or printing (→ PDF).
// Both start from the note as the preview renders it (already sanitized).
import appCss from "../app.css?raw";
import { escapeHtml, render } from "./markdown";
import { renderMermaid } from "./mermaid";

/**
 * A detached copy of the rendered note. Uses the open preview when there is
 * one (diagrams already drawn); otherwise renders the source off-screen.
 */
export async function renderedCopy(article: HTMLElement | undefined, source: string): Promise<HTMLElement> {
  if (article) return article.cloneNode(true) as HTMLElement;
  const host = document.createElement("article");
  host.className = "markdown";
  host.style.cssText = "position:fixed;left:-10000px;top:0;width:800px;visibility:hidden";
  host.innerHTML = render(source);
  document.body.appendChild(host);
  try {
    await renderMermaid(host);
  } finally {
    host.remove();
  }
  return host;
}

/** Removes the app's own controls and bookkeeping attributes. */
function clean(root: HTMLElement) {
  root.querySelectorAll(".code-tools").forEach((el) => el.remove());
  root.querySelectorAll("[data-line]").forEach((el) => el.removeAttribute("data-line"));
  root.querySelectorAll("a[data-title]").forEach((a) => {
    a.setAttribute("title", a.getAttribute("data-title")!);
    a.removeAttribute("data-title");
  });
}

/** A complete, self-contained HTML document for the note. */
export function htmlDocument(title: string, rendered: HTMLElement): string {
  const root = rendered.cloneNode(true) as HTMLElement;
  clean(root);
  // Images: point back at the vault; the backend embeds them as data: URLs.
  root.querySelectorAll<HTMLImageElement>("img[data-attachment]").forEach((img) => {
    img.setAttribute("src", `attachment:${img.dataset.attachment}`);
    img.removeAttribute("data-attachment");
    img.removeAttribute("loading");
  });
  // Other attachments (PDFs, ...) aren't included: keep just their name.
  root.querySelectorAll("a.attachment-link").forEach((a) => {
    const span = document.createElement("span");
    span.className = "attachment-link";
    span.textContent = a.textContent;
    span.title = "Attachment not included in this export";
    a.replaceWith(span);
  });
  // Math: keep KaTeX's MathML (browsers draw it natively), drop the HTML
  // version that needs KaTeX's CSS and fonts.
  root.querySelectorAll(".katex-html").forEach((el) => el.remove());

  // The file loads nothing from anywhere: no scripts, no remote resources.
  const csp = "default-src 'none'; img-src data:; style-src 'unsafe-inline'";
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)}</title>
<style>
${appCss}
html, body { height: auto; overflow: auto; }
.markdown .katex-mathml { font-size: 1.1em; }
.markdown .tag { cursor: default; }
</style>
</head>
<body>
<article class="markdown">
${root.innerHTML}
</article>
</body>
</html>
`;
}

/**
 * Opens the system print dialog with just the note (Print to File → PDF).
 * `printRoot` is an element shown only when printing.
 */
export async function printNote(printRoot: HTMLElement, rendered: HTMLElement): Promise<void> {
  const root = rendered.cloneNode(true) as HTMLElement;
  clean(root);
  printRoot.replaceChildren(...root.childNodes);
  // Wait for images so they make it onto the page.
  await Promise.all([...printRoot.querySelectorAll("img")].map((img) => img.decode().catch(() => {})));
  const done = () => {
    printRoot.replaceChildren();
    window.removeEventListener("afterprint", done);
  };
  window.addEventListener("afterprint", done);
  window.print();
}

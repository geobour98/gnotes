// Renders ```mermaid placeholders in the preview. Mermaid is large, so it is
// loaded only when a note actually contains a diagram.
import DOMPurify from "dompurify";
import type { Mermaid } from "mermaid";
import { escapeHtml } from "./markdown";

let loading: Promise<Mermaid> | null = null;
let counter = 0;
const cache = new Map<string, string>(); // diagram source -> sanitized SVG

function load(): Promise<Mermaid> {
  loading ??= import("mermaid").then(({ default: mermaid }) => {
    const dark = matchMedia("(prefers-color-scheme: dark)").matches;
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict", // no clicks/scripts inside diagrams
      theme: dark ? "dark" : "default",
      htmlLabels: false, // pure SVG text, no <foreignObject> HTML
      flowchart: { htmlLabels: false },
      fontFamily: "system-ui, sans-serif",
    });
    return mermaid;
  });
  return loading;
}

async function toSvg(mermaid: Mermaid, source: string): Promise<string> {
  const id = `gnotes-mermaid-${++counter}`;
  try {
    const { svg } = await mermaid.render(id, source);
    return DOMPurify.sanitize(svg, {
      USE_PROFILES: { svg: true, svgFilters: true },
      ADD_TAGS: ["style"], // diagram-scoped CSS emitted by mermaid
    }) as string;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return `<pre class="mermaid-error">Diagram error: ${escapeHtml(msg)}</pre>`;
  } finally {
    // Mermaid can leave temporary nodes behind on errors.
    document.getElementById(id)?.remove();
    document.getElementById(`d${id}`)?.remove();
  }
}

export async function renderMermaid(root: HTMLElement): Promise<void> {
  const blocks = [...root.querySelectorAll<HTMLElement>("div.mermaid:not([data-rendered])")];
  if (!blocks.length) return;
  const mermaid = await load();
  for (const el of blocks) {
    const source = el.textContent ?? "";
    let svg = cache.get(source);
    if (svg === undefined) {
      svg = await toSvg(mermaid, source);
      if (cache.size > 200) cache.clear();
      cache.set(source, svg);
    }
    if (el.isConnected) {
      el.innerHTML = svg;
      el.dataset.rendered = "";
    }
  }
}

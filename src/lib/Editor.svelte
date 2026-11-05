<script lang="ts" module>
  import type { EditorState as State } from "@codemirror/state";
  /** Everything needed to resume editing a note: text, cursor, undo history, scroll. */
  export type EditorSnapshot = { state: State; scrollTop: number };
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { EditorView, keymap, placeholder } from "@codemirror/view";
  import { EditorState, StateEffect } from "@codemirror/state";
  import { basicSetup } from "codemirror";
  import { indentWithTab } from "@codemirror/commands";
  import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
  import { languages } from "@codemirror/language-data";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { tags as t } from "@lezer/highlight";

  let {
    doc,
    initial,
    onchange,
    onscrollline,
    onpastefiles,
    onsnapshot,
  }: {
    doc: string;
    /** Resume from a snapshot (used instead of `doc`). */
    initial?: EditorSnapshot;
    /** Receives the editor's state when it is destroyed (tab switch, view change). */
    onsnapshot?: (s: EditorSnapshot) => void;
    /** Files pasted into the editor (e.g. a screenshot). */
    onpastefiles?: (files: File[]) => void;
    onchange: (text: string) => void;
    /** Fired on scroll with the 0-based (fractional) line at the top. */
    onscrollline?: (line: number) => void;
  } = $props();

  let host: HTMLDivElement;
  let view: EditorView | undefined;

  // Colors come from CSS variables so light/dark themes just work.
  const highlight = HighlightStyle.define([
    { tag: t.heading1, fontSize: "1.5em", fontWeight: "700", color: "var(--heading)" },
    { tag: t.heading2, fontSize: "1.3em", fontWeight: "700", color: "var(--heading)" },
    { tag: t.heading3, fontSize: "1.15em", fontWeight: "700", color: "var(--heading)" },
    { tag: [t.heading4, t.heading5, t.heading6], fontWeight: "700", color: "var(--heading)" },
    { tag: t.strong, fontWeight: "700" },
    { tag: t.emphasis, fontStyle: "italic" },
    { tag: t.strikethrough, textDecoration: "line-through" },
    { tag: [t.link, t.url], color: "var(--accent)" },
    { tag: t.monospace, fontFamily: "var(--mono)", color: "var(--code)" },
    { tag: [t.processingInstruction, t.contentSeparator, t.meta], color: "var(--muted)" },
    { tag: t.quote, color: "var(--muted)", fontStyle: "italic" },
    { tag: t.keyword, color: "var(--syn-keyword)" },
    { tag: [t.string, t.special(t.string)], color: "var(--syn-string)" },
    { tag: [t.number, t.bool, t.null], color: "var(--syn-number)" },
    { tag: t.comment, color: "var(--muted)", fontStyle: "italic" },
    { tag: [t.function(t.variableName), t.typeName, t.className], color: "var(--syn-fn)" },
  ]);

  const theme = EditorView.theme({
    "&": { height: "100%", backgroundColor: "transparent", color: "var(--fg)" },
    ".cm-scroller": { fontFamily: "var(--editor-font)", lineHeight: "1.6", padding: "1rem 0" },
    ".cm-content": { maxWidth: "52rem", margin: "0 auto", padding: "0 1.5rem", caretColor: "var(--accent)" },
    ".cm-gutters": { display: "none" },
    ".cm-activeLine": { backgroundColor: "transparent" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
      backgroundColor: "var(--selection) !important",
    },
    ".cm-cursor": { borderLeftColor: "var(--accent)" },
    "&.cm-focused": { outline: "none" },
  });

  /** Position of document top inside the scroll content (i.e. padding). */
  function docOffset(v: EditorView): number {
    return v.documentTop - v.scrollDOM.getBoundingClientRect().top + v.scrollDOM.scrollTop;
  }

  /** Fractional 0-based line at the top of the viewport. */
  export function topLine(): number {
    if (!view) return 0;
    const h = view.scrollDOM.scrollTop - docOffset(view);
    const block = view.lineBlockAtHeight(Math.max(0, h));
    const line = view.state.doc.lineAt(block.from).number - 1;
    const frac = block.height > 0 ? (h - block.top) / block.height : 0;
    return line + Math.min(Math.max(frac, 0), 1);
  }

  /** Scrolls so the given fractional 0-based line is at the top. */
  /** Jumps to the very top or bottom (the cursor stays where it is). */
  export function scrollToEdge(edge: "top" | "bottom") {
    if (view) view.scrollDOM.scrollTop = edge === "top" ? 0 : view.scrollDOM.scrollHeight;
  }

  /**
   * Centers 0-based `line`, selects the first of `terms` found on it (or
   * puts the cursor at its start) and focuses the editor.
   */
  export function revealLine(line: number, terms: string[] = []) {
    if (!view) return;
    const text = view.state.doc;
    const l = text.line(Math.min(Math.max(Math.floor(line), 0), text.lines - 1) + 1);
    let from = l.from;
    let to = l.from;
    const lower = l.text.toLowerCase();
    if (lower.length === l.text.length) {
      let best = -1;
      for (const term of terms) {
        const i = lower.indexOf(term);
        if (i >= 0 && (best < 0 || i < best)) [best, from, to] = [i, l.from + i, l.from + i + term.length];
      }
    }
    view.dispatch({ selection: { anchor: from, head: to }, effects: EditorView.scrollIntoView(from, { y: "center" }) });
    view.focus();
  }

  export function scrollToLine(line: number) {
    if (!view) return;
    const text = view.state.doc;
    const n = Math.min(Math.max(Math.floor(line), 0), text.lines - 1);
    const block = view.lineBlockAt(text.line(n + 1).from);
    view.scrollDOM.scrollTop = docOffset(view) + block.top + (line - n) * block.height;
  }

  /** Inserts text at the cursor (replacing any selection) and focuses. */
  export function insertText(text: string) {
    if (!view) return;
    const { from, to } = view.state.selection.main;
    view.dispatch({
      changes: { from, to, insert: text },
      selection: { anchor: from + text.length },
      scrollIntoView: true,
    });
    view.focus();
  }

  /** Replaces one line (0-based) without moving the cursor. */
  export function replaceLine(line: number, text: string) {
    if (!view || line < 0 || line >= view.state.doc.lines) return;
    const l = view.state.doc.line(line + 1);
    view.dispatch({ changes: { from: l.from, to: l.to, insert: text } });
  }

  // Built fresh for every editor instance: the callbacks belong to it.
  function extensions() {
    return [
      basicSetup,
      keymap.of([indentWithTab]),
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      syntaxHighlighting(highlight),
      EditorView.lineWrapping,
      placeholder("Start writing…"),
      theme,
      EditorView.domEventHandlers({
        paste(e) {
          const files = [...(e.clipboardData?.files ?? [])];
          if (!files.length || !onpastefiles) return false;
          e.preventDefault();
          onpastefiles(files);
          return true;
        },
      }),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) onchange(u.state.doc.toString());
      }),
    ];
  }

  onMount(() => {
    // A snapshot keeps its text, selection and undo history; only the
    // extensions (whose callbacks point at the old instance) are replaced.
    const state = initial
      ? initial.state.update({ effects: StateEffect.reconfigure.of(extensions()) }).state
      : EditorState.create({ doc, extensions: extensions() });
    view = new EditorView({ parent: host, state });
    if (initial) {
      const top = initial.scrollTop;
      requestAnimationFrame(() => view && (view.scrollDOM.scrollTop = top));
    }
    view.scrollDOM.addEventListener("scroll", () => onscrollline?.(topLine()), { passive: true });
    view.focus();
  });

  onDestroy(() => {
    if (view) onsnapshot?.({ state: view.state, scrollTop: view.scrollDOM.scrollTop });
    view?.destroy();
  });
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    height: 100%;
    overflow: hidden;
  }
</style>

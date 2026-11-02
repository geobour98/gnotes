<script lang="ts">
  // Full-text search panel. The search itself runs in Rust, inside the
  // unlocked vault; this only shows results. Text is rendered as plain
  // Svelte text (never as HTML), so note contents can't inject markup.
  import { untrack } from "svelte";
  import { api, type SearchHit, type SearchResults } from "./api";

  let {
    visible,
    selectedId,
    onopen,
    onclose,
  }: {
    visible: boolean;
    selectedId: string | null;
    /** Open a note, optionally at a 0-based line, highlighting `terms`. */
    onopen: (hit: SearchHit, line: number | null, terms: string[]) => void;
    onclose: () => void;
  } = $props();

  let query = $state("");
  let results = $state<SearchResults | null>(null);
  let error = $state("");
  let input = $state<HTMLInputElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0; // ignore answers to queries that were already replaced

  async function run() {
    clearTimeout(timer);
    const q = query;
    const mine = ++seq;
    if (!q.trim()) {
      results = null;
      error = "";
      return;
    }
    try {
      const r = await api.search(q);
      if (mine === seq) [results, error] = [r, ""];
    } catch (e) {
      if (mine === seq) error = String(e);
    }
  }

  function onInput() {
    clearTimeout(timer);
    timer = setTimeout(run, 200);
  }

  /** Focuses the box and selects its text (Ctrl+Shift+F). */
  export function focus() {
    input?.focus();
    input?.select();
  }

  // Notes may have changed while the panel was hidden: search again.
  // (untrack: only `visible` should re-run this, not typing.)
  $effect(() => {
    if (visible) untrack(run);
  });

  type Part = { text: string; mark: boolean };

  /** Splits `text` into plain and highlighted parts (case-insensitive). */
  function highlight(text: string, terms: string[]): Part[] {
    const lower = text.toLowerCase();
    // Lowercasing changed the length (rare scripts): skip highlighting.
    if (lower.length !== text.length || terms.length === 0) return [{ text, mark: false }];
    const marked = new Uint8Array(text.length);
    for (const t of terms) {
      for (let i = lower.indexOf(t); i >= 0 && t; i = lower.indexOf(t, i + t.length)) {
        marked.fill(1, i, i + t.length);
      }
    }
    const parts: Part[] = [];
    let start = 0;
    for (let i = 1; i <= text.length; i++) {
      if (i === text.length || marked[i] !== marked[start]) {
        parts.push({ text: text.slice(start, i), mark: marked[start] === 1 });
        start = i;
      }
    }
    return parts;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      if (query) {
        query = "";
        run();
      } else onclose();
    } else if (e.key === "Enter" && results?.hits.length) {
      clearTimeout(timer);
      const hit = results.hits[0];
      onopen(hit, hit.lines[0]?.line ?? null, results.terms);
    }
  }
</script>

<div class="search" hidden={!visible}>
  <div class="box">
    <input
      bind:this={input}
      bind:value={query}
      oninput={onInput}
      onkeydown={onKeydown}
      type="search"
      placeholder="Search notes…"
      spellcheck="false"
      aria-label="Search notes"
    />
  </div>
  <div class="summary">
    {#if error}
      <span class="error">{error}</span>
    {:else if results}
      {results.total === 0
        ? "No matches"
        : results.total > results.hits.length
          ? `${results.total} notes (showing ${results.hits.length})`
          : `${results.total} note${results.total === 1 ? "" : "s"}`}
    {:else}
      All words must match. Use "quotes" for a phrase.
    {/if}
  </div>
  {#if results}
    {@const terms = results.terms}
    <div class="results">
      {#each results.hits as hit (hit.id)}
        <div class="hit" class:active={hit.id === selectedId}>
          <button class="title" onclick={() => onopen(hit, hit.lines[0]?.line ?? null, terms)}>
            <span class="name">
              {#each highlight(hit.title, terms) as p}{#if p.mark}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}
            </span>
            {#if hit.folder}<span class="folder">{hit.folder}</span>{/if}
          </button>
          {#each hit.lines as m (m.line)}
            <button class="line" title="Line {m.line + 1}" onclick={() => onopen(hit, m.line, terms)}>
              <span class="ln">{m.line + 1}</span>
              <span class="text">
                {#each highlight(m.text, terms) as p}{#if p.mark}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}
              </span>
            </button>
          {/each}
          {#if hit.line_count > hit.lines.length}
            <div class="more">+{hit.line_count - hit.lines.length} more lines</div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }
  .search[hidden] {
    display: none;
  }
  .box {
    padding: 0.5rem 0.5rem 0.25rem;
  }
  .box input {
    width: 100%;
    box-sizing: border-box;
  }
  .summary {
    padding: 0 0.75rem 0.4rem;
    font-size: 0.78rem;
    color: var(--muted);
  }
  .error {
    color: var(--danger);
  }
  .results {
    overflow-y: auto;
    flex: 1;
    padding-bottom: 1rem;
  }
  .hit {
    margin: 0 0.35rem 0.35rem;
    border-radius: 6px;
  }
  .hit.active {
    background: var(--selection);
  }
  button {
    display: flex;
    width: 100%;
    text-align: left;
    border: none;
    background: none;
    border-radius: 6px;
    padding: 0.2rem 0.5rem;
    color: inherit;
    cursor: pointer;
    min-width: 0;
  }
  button:hover {
    background: var(--hover);
  }
  .title {
    flex-direction: column;
    gap: 0.05rem;
    padding-top: 0.3rem;
  }
  .name {
    font-weight: 600;
    font-size: 0.9rem;
  }
  .folder {
    font-size: 0.75rem;
    color: var(--muted);
  }
  .name,
  .folder,
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .line {
    gap: 0.45rem;
    font-size: 0.8rem;
    padding-left: 0.75rem;
  }
  .ln {
    color: var(--muted);
    min-width: 1.6rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .text {
    flex: 1;
  }
  .more {
    font-size: 0.75rem;
    color: var(--muted);
    padding: 0 0.5rem 0.2rem 2.85rem;
  }
  mark {
    background: color-mix(in srgb, #ffd54a 55%, transparent);
    color: inherit;
    border-radius: 2px;
  }
</style>

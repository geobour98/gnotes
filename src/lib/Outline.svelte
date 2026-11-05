<script lang="ts" module>
  export type Heading = { level: number; text: string; line: number };

  const FRONT_MATTER = /^---[ \t]*\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/;

  /** Makes heading source readable: drops emphasis, code and link syntax. */
  function plain(text: string): string {
    return text
      .replace(/\s+#+\s*$/, "") // closing hashes: "## Title ##"
      .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1") // [text](url), ![alt](src)
      .replace(/(\*\*|__|\*|_|~~|==|`)(.+?)\1/g, "$2")
      .trim();
  }

  /** ATX headings (# … ######), outside front matter and fenced code. */
  export function parseHeadings(source: string): Heading[] {
    const out: Heading[] = [];
    const fm = FRONT_MATTER.exec(source);
    const lines = source.split("\n");
    let i = fm ? fm[0].split("\n").length - 1 : 0;
    let fence: { ch: string; n: number } | null = null;
    for (; i < lines.length; i++) {
      const line = lines[i];
      const f = /^ {0,3}(`{3,}|~{3,})/.exec(line);
      if (fence) {
        if (f && f[1][0] === fence.ch && f[1].length >= fence.n && !line.slice(f[0].length).trim()) fence = null;
        continue;
      }
      if (f) {
        fence = { ch: f[1][0], n: f[1].length };
        continue;
      }
      const h = /^ {0,3}(#{1,6})[ \t]+(.+?)\s*$/.exec(line);
      if (h) {
        const text = plain(h[2]);
        if (text) out.push({ level: h[1].length, text, line: i });
      }
    }
    return out;
  }
</script>

<script lang="ts">
  let {
    source,
    topLine,
    onjump,
    onclose,
  }: {
    source: string;
    /** Source line currently at the top of the view (highlights its section). */
    topLine: number;
    onjump: (line: number) => void;
    onclose: () => void;
  } = $props();

  let headings = $derived(parseHeadings(source));
  // Collapsed headings, keyed by level + text (stable while editing).
  let collapsed = $state<Record<string, boolean>>({});
  const key = (h: Heading) => `${h.level}:${h.text}`;

  /** Index of the section the view is in (last heading at or above topLine). */
  let active = $derived.by(() => {
    let idx = -1;
    for (let i = 0; i < headings.length && headings[i].line <= Math.floor(topLine) + 1; i++) idx = i;
    return idx;
  });

  type Row = { h: Heading; index: number; hasChildren: boolean; depth: number };

  let rows = $derived.by(() => {
    const out: Row[] = [];
    const stack: Heading[] = []; // ancestors of the current heading
    let hiddenBelow = Infinity; // headings deeper than this level are hidden
    headings.forEach((h, index) => {
      while (stack.length && stack[stack.length - 1].level >= h.level) stack.pop();
      if (h.level <= hiddenBelow) hiddenBelow = Infinity;
      const hasChildren = headings[index + 1]?.level > h.level;
      if (hiddenBelow === Infinity) {
        out.push({ h, index, hasChildren, depth: stack.length });
        if (hasChildren && collapsed[key(h)]) hiddenBelow = h.level;
      }
      stack.push(h);
    });
    return out;
  });

  function setAll(value: boolean) {
    const next: Record<string, boolean> = {};
    if (value) for (const h of headings) next[key(h)] = true;
    collapsed = next;
  }
</script>

<aside class="outline" aria-label="Outline">
  <header>
    <span class="title">Outline</span>
    <span class="spacer"></span>
    <button title="Collapse all" aria-label="Collapse all" onclick={() => setAll(true)}>⊟</button>
    <button title="Expand all" aria-label="Expand all" onclick={() => setAll(false)}>⊞</button>
    <button title="Hide outline (Ctrl+Shift+O)" aria-label="Hide outline" onclick={onclose}>✕</button>
  </header>
  <div class="list" role="tree">
    {#if headings.length === 0}
      <p class="none">No headings. Start a line with <code>#</code>, <code>##</code>, …</p>
    {/if}
    {#each rows as r (r.index)}
      {@const isCollapsed = !!collapsed[key(r.h)]}
      <div
        class="row"
        class:active={r.index === active}
        style:padding-left="{0.35 + r.depth * 0.85}rem"
        role="treeitem"
        aria-selected={r.index === active}
        aria-expanded={r.hasChildren ? !isCollapsed : undefined}
        tabindex="0"
        title={r.h.text}
        onclick={() => onjump(r.h.line)}
        onkeydown={(e) => e.key === "Enter" && onjump(r.h.line)}
      >
        {#if r.hasChildren}
          <button
            class="chev"
            class:open={!isCollapsed}
            aria-label={isCollapsed ? "Expand" : "Collapse"}
            onclick={(e) => (e.stopPropagation(), (collapsed[key(r.h)] = !isCollapsed))}>›</button
          >
        {:else}
          <span class="chev"></span>
        {/if}
        <span class="text h{r.h.level}">{r.h.text}</span>
      </div>
    {/each}
  </div>
</aside>

<style>
  .outline {
    display: flex;
    flex-direction: column;
    min-height: 0;
    width: 15rem;
    flex-shrink: 0;
    border-left: 1px solid var(--border);
    background: var(--bg-side);
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.1rem;
    padding: 0.4rem 0.4rem 0.4rem 0.85rem;
    border-bottom: 1px solid var(--border);
  }
  .title {
    color: var(--muted);
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .spacer {
    flex: 1;
  }
  header button {
    border: none;
    background: none;
    color: var(--muted);
    padding: 0.1rem 0.35rem;
    font-size: 0.85rem;
  }
  header button:hover {
    color: var(--fg);
  }
  .list {
    overflow-y: auto;
    flex: 1;
    padding: 0.35rem 0 1rem;
  }
  .none {
    margin: 0.3rem 0.85rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.18rem 0.5rem;
    margin: 0 0.35rem;
    border-radius: 6px;
    cursor: pointer;
    user-select: none;
    font-size: 0.85rem;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.active {
    background: var(--selection);
  }
  .chev {
    display: inline-block;
    width: 0.8rem;
    flex-shrink: 0;
    border: none;
    background: none;
    padding: 0;
    color: var(--muted);
    font-size: inherit;
    cursor: pointer;
    transition: transform 0.1s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .text {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text.h1 {
    font-weight: 600;
  }
  .text.h4,
  .text.h5,
  .text.h6 {
    color: var(--muted);
  }
</style>

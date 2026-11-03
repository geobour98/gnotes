<script lang="ts">
  // The "Tags" section under the notes: every tag with its note count, nested
  // tags (#ai/llm) shown as a tree. Clicking a tag searches for it.
  import type { TagCount } from "./api";

  type Node = { tag: string; name: string; count: number; children: Node[] };

  let {
    tags,
    activeTag,
    ontag,
  }: {
    tags: TagCount[];
    /** The tag currently searched for, if any (highlighted). */
    activeTag: string | null;
    ontag: (tag: string) => void;
  } = $props();

  let open = $state(loadOpen());
  // Nested tags start collapsed; this holds the ones the user opened.
  let expanded = $state<Record<string, boolean>>({});

  function loadOpen(): boolean {
    try {
      return localStorage.getItem("gnotes.tagsOpen") !== "0";
    } catch {
      return true;
    }
  }
  function toggleOpen() {
    open = !open;
    try {
      localStorage.setItem("gnotes.tagsOpen", open ? "1" : "0");
    } catch {}
  }

  function build(list: TagCount[]): Node[] {
    const roots: Node[] = [];
    const byTag = new Map<string, Node>();
    // Sorted input: parents (always included) come before their children.
    for (const { tag, count } of list) {
      const i = tag.lastIndexOf("/");
      const node: Node = { tag, name: tag.slice(i + 1), count, children: [] };
      byTag.set(tag, node);
      (i < 0 ? roots : (byTag.get(tag.slice(0, i))?.children ?? roots)).push(node);
    }
    return roots;
  }

  let roots = $derived(build(tags));

  // Show where the active tag is, e.g. after clicking #ai/llm in the preview.
  $effect(() => {
    if (!activeTag) return;
    const parts = activeTag.split("/");
    for (let i = 1; i < parts.length; i++) expanded[parts.slice(0, i).join("/")] = true;
  });
</script>

<section class="tags" class:open>
  <button class="head" aria-expanded={open} onclick={toggleOpen}>
    <span class="chev" class:open>›</span>
    Tags
    <span class="n">{tags.filter((t) => !t.tag.includes("/")).length || ""}</span>
  </button>
  {#if open}
    <div class="list" role="tree">
      {#if roots.length === 0}
        <p class="none">No tags yet. Write <code>#tag</code> in a note.</p>
      {/if}
      {@render level(roots, 0)}
    </div>
  {/if}
</section>

{#snippet level(nodes: Node[], depth: number)}
  {#each nodes as n (n.tag)}
    {@const isOpen = !!expanded[n.tag]}
    <div
      class="row"
      class:active={activeTag === n.tag}
      style:padding-left="{0.5 + depth * 0.9}rem"
      role="treeitem"
      aria-selected={activeTag === n.tag}
      aria-expanded={n.children.length ? isOpen : undefined}
      tabindex="0"
      title="Show notes tagged #{n.tag}"
      onclick={() => ontag(n.tag)}
      onkeydown={(e) => e.key === "Enter" && ontag(n.tag)}
    >
      {#if n.children.length}
        <button
          class="chev"
          class:open={isOpen}
          aria-label={isOpen ? "Collapse" : "Expand"}
          onclick={(e) => (e.stopPropagation(), (expanded[n.tag] = !isOpen))}>›</button
        >
      {:else}
        <span class="chev"></span>
      {/if}
      <span class="name">#{n.name}</span>
      <span class="count">{n.count}</span>
    </div>
    {#if isOpen}{@render level(n.children, depth + 1)}{/if}
  {/each}
{/snippet}

<style>
  .tags {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-top: 1px solid var(--border);
  }
  .tags.open {
    max-height: 40%;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    background: none;
    color: var(--muted);
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0.45rem 0.85rem;
    cursor: pointer;
    text-align: left;
  }
  .head:hover {
    color: var(--fg);
  }
  .n {
    font-weight: 400;
  }
  .list {
    overflow-y: auto;
    padding-bottom: 0.5rem;
  }
  .none {
    margin: 0.2rem 0.85rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.22rem 0.5rem;
    margin: 0 0.35rem;
    border-radius: 6px;
    cursor: pointer;
    user-select: none;
    font-size: 0.88rem;
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
    color: var(--muted);
    transition: transform 0.1s;
    border: none;
    background: none;
    padding: 0;
    font-size: inherit;
    cursor: pointer;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    font-size: 0.75rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>

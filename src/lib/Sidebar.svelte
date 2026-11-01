<script lang="ts">
  import type { NoteMeta, Tree } from "./api";

  type FolderNode = { path: string; name: string; folders: FolderNode[]; notes: NoteMeta[] };

  let {
    tree,
    selectedId,
    selectedFolder = $bindable(""),
    onopen,
    onrenameNote,
    ondeleteNote,
    onrenameFolder,
    ondeleteFolder,
  }: {
    tree: Tree;
    selectedId: string | null;
    selectedFolder: string;
    onopen: (n: NoteMeta) => void;
    onrenameNote: (n: NoteMeta) => void;
    ondeleteNote: (n: NoteMeta) => void;
    onrenameFolder: (path: string) => void;
    ondeleteFolder: (path: string) => void;
  } = $props();

  let collapsed = $state<Record<string, boolean>>({});
  let filter = $state("");

  function build(t: Tree, q: string): FolderNode {
    const root: FolderNode = { path: "", name: "", folders: [], notes: [] };
    const byPath = new Map([["", root]]);
    for (const p of t.folders) {
      const i = p.lastIndexOf("/");
      const node: FolderNode = { path: p, name: p.slice(i + 1), folders: [], notes: [] };
      byPath.set(p, node);
      byPath.get(i < 0 ? "" : p.slice(0, i))?.folders.push(node);
    }
    const needle = q.trim().toLowerCase();
    for (const n of t.notes) {
      if (needle && !n.title.toLowerCase().includes(needle)) continue;
      byPath.get(n.folder)?.notes.push(n);
    }
    return root;
  }

  function hasNotes(f: FolderNode): boolean {
    return f.notes.length > 0 || f.folders.some(hasNotes);
  }

  let root = $derived(build(tree, filter));
</script>

<div class="sidebar">
  <input class="filter" placeholder="Filter notes…" bind:value={filter} spellcheck="false" />
  <div class="items" role="tree">
    {@render folder(root, 0)}
  </div>
</div>

{#snippet folder(node: FolderNode, depth: number)}
  {#each node.folders as f (f.path)}
    {#if !filter || hasNotes(f)}
      {@const open = filter ? true : !collapsed[f.path]}
      <div
        class="row folder"
        class:active={selectedFolder === f.path}
        style:padding-left="{0.5 + depth * 0.9}rem"
        role="treeitem"
        aria-selected={selectedFolder === f.path}
        aria-expanded={open}
        tabindex="0"
        onclick={() => {
          selectedFolder = f.path;
          collapsed[f.path] = open;
        }}
        onkeydown={(e) => e.key === "Enter" && (collapsed[f.path] = open)}
      >
        <span class="chev" class:open>›</span>
        <span class="name">{f.name}</span>
        <span class="actions">
          <button title="Rename / move" onclick={(e) => (e.stopPropagation(), onrenameFolder(f.path))}>✎</button>
          <button title="Delete" onclick={(e) => (e.stopPropagation(), ondeleteFolder(f.path))}>✕</button>
        </span>
      </div>
      {#if open}{@render folder(f, depth + 1)}{/if}
    {/if}
  {/each}
  {#each node.notes as n (n.id)}
    <div
      class="row note"
      class:active={selectedId === n.id}
      style:padding-left="{1.4 + depth * 0.9}rem"
      role="treeitem"
      aria-selected={selectedId === n.id}
      tabindex="0"
      onclick={() => {
        selectedFolder = n.folder;
        onopen(n);
      }}
      onkeydown={(e) => e.key === "Enter" && onopen(n)}
    >
      <span class="name">{n.title}</span>
      <span class="actions">
        <button title="Rename / move" onclick={(e) => (e.stopPropagation(), onrenameNote(n))}>✎</button>
        <button title="Delete" onclick={(e) => (e.stopPropagation(), ondeleteNote(n))}>✕</button>
      </span>
    </div>
  {/each}
{/snippet}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .filter {
    margin: 0.5rem;
  }
  .items {
    overflow-y: auto;
    flex: 1;
    padding-bottom: 1rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.28rem 0.5rem;
    cursor: pointer;
    border-radius: 6px;
    margin: 0 0.35rem;
    user-select: none;
    font-size: 0.92rem;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.active {
    background: var(--selection);
  }
  .folder .name {
    font-weight: 600;
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    display: inline-block;
    width: 0.8rem;
    color: var(--muted);
    transition: transform 0.1s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .actions {
    display: none;
    gap: 0.1rem;
  }
  .row:hover .actions {
    display: flex;
  }
  .actions button {
    border: none;
    background: none;
    padding: 0 0.25rem;
    color: var(--muted);
    font-size: 0.8rem;
  }
  .actions button:hover {
    color: var(--fg);
  }
</style>

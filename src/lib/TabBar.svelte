<script lang="ts">
  import type { NoteMeta } from "./api";

  let {
    tabs,
    active,
    dirty,
    onselect,
    onclose,
  }: {
    tabs: NoteMeta[];
    /** Index of the active tab, -1 when none (start view). */
    active: number;
    /** The active tab has unsaved edits. */
    dirty: boolean;
    onselect: (index: number) => void;
    onclose: (index: number) => void;
  } = $props();

  let bar = $state<HTMLDivElement>();

  // Keep the active tab in view when there are more tabs than fit.
  $effect(() => {
    void tabs.length;
    const el = bar?.children[active] as HTMLElement | undefined;
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });

  // A vertical wheel scrolls the tab strip sideways (non-passive listener,
  // so the page itself doesn't scroll).
  $effect(() => {
    const el = bar;
    if (!el) return;
    const onwheel = (e: WheelEvent) => {
      if (!e.ctrlKey && Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
        el.scrollLeft += e.deltaY;
        e.preventDefault();
      }
    };
    el.addEventListener("wheel", onwheel, { passive: false });
    return () => el.removeEventListener("wheel", onwheel);
  });
</script>

<div class="tabs" role="tablist" aria-label="Open notes" bind:this={bar}>
  {#each tabs as t, i (t.id)}
    <div
      class="tab"
      class:active={i === active}
      role="tab"
      aria-selected={i === active}
      tabindex={i === active ? 0 : -1}
      title={t.folder ? `${t.folder} / ${t.title}` : t.title}
      onclick={() => onselect(i)}
      onkeydown={(e) => e.key === "Enter" && onselect(i)}
      onmousedown={(e) => e.button === 1 && e.preventDefault()}
      onauxclick={(e) => e.button === 1 && (e.preventDefault(), onclose(i))}
    >
      <span class="name">{t.title}</span>
      <button
        class="close"
        class:dirty={i === active && dirty}
        title="Close (Ctrl+W)"
        aria-label="Close {t.title}"
        onclick={(e) => (e.stopPropagation(), onclose(i))}
      >
        <span class="dot">●</span><span class="x">✕</span>
      </button>
    </div>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    min-height: 2.2rem;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
    background: var(--bg-side);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    max-width: 13rem;
    min-width: 6rem;
    padding: 0 0.35rem 0 0.75rem;
    border-right: 1px solid var(--border);
    font-size: 0.85rem;
    color: var(--muted);
    cursor: pointer;
    user-select: none;
    flex-shrink: 0;
  }
  .tab:hover {
    background: var(--hover);
  }
  .tab.active {
    background: var(--bg);
    color: var(--fg);
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    border: none;
    background: none;
    padding: 0.05rem 0.3rem;
    border-radius: 4px;
    color: var(--muted);
    font-size: 0.75rem;
    line-height: 1;
    visibility: hidden;
  }
  .tab:hover .close,
  .tab.active .close,
  .close.dirty {
    visibility: visible;
  }
  .close:hover {
    background: var(--border);
    color: var(--fg);
  }
  /* Unsaved: a dot that turns into ✕ on hover, like most editors. */
  .dot,
  .close.dirty:hover .dot {
    display: none;
  }
  .close.dirty .dot {
    display: inline;
  }
  .close.dirty .x {
    display: none;
  }
  .close.dirty:hover .x {
    display: inline;
  }
</style>

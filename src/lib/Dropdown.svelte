<script lang="ts" module>
  /** A menu entry, or "-" for a separator line. */
  export type MenuItem = { label: string; run: () => unknown; checked?: boolean } | "-";
</script>

<script lang="ts">
  // A button that opens a small menu. Closes on a choice, Escape, or a click
  // anywhere else.
  let {
    label,
    title,
    items,
    up = false,
    variant = "tool",
  }: {
    label: string;
    title?: string;
    items: MenuItem[];
    /** Open above the button (e.g. in a footer) instead of below. */
    up?: boolean;
    /** "tool": small header button; "block": fills its container. */
    variant?: "tool" | "block";
  } = $props();

  let open = $state(false);
  let wrap = $state<HTMLDivElement>();

  function choose(run: () => unknown) {
    open = false;
    run();
  }
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && (open = false)}
  onmousedown={(e) => open && !wrap?.contains(e.target as Node) && (open = false)}
/>

<div class="wrap {variant}" bind:this={wrap}>
  <button class="trigger" aria-haspopup="menu" aria-expanded={open} {title} onclick={() => (open = !open)}>
    {label}
  </button>
  {#if open}
    <div class="menu" class:up role="menu">
      {#each items as item}
        {#if item === "-"}
          <hr />
        {:else if item.checked !== undefined}
          <button role="menuitemcheckbox" aria-checked={item.checked} onclick={() => choose(item.run)}>
            <span class="check">{item.checked ? "✓" : ""}</span>{item.label}
          </button>
        {:else}
          <button role="menuitem" onclick={() => choose(item.run)}>{item.label}</button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .tool .trigger {
    font-size: 0.8rem;
    padding: 0.2rem 0.55rem;
    margin-right: 0.35rem;
  }
  .block .trigger {
    width: 100%;
    font-size: 0.85rem;
    padding: 0.35rem 0.5rem;
  }
  .menu {
    position: absolute;
    top: calc(100% + 0.35rem);
    right: 0;
    min-width: 14rem;
    display: flex;
    flex-direction: column;
    padding: 0.3rem;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.3);
    z-index: 20;
  }
  .menu.up {
    top: auto;
    bottom: calc(100% + 0.35rem);
    right: auto;
    left: 0;
  }
  .menu button {
    display: flex;
    border: none;
    background: none;
    text-align: left;
    padding: 0.4rem 0.6rem;
    font-size: 0.88rem;
    border-radius: 5px;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .check {
    display: inline-block;
    width: 1.2em;
  }
  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 0.25rem 0;
  }
</style>

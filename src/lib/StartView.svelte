<script lang="ts">
  // Shown when no note is open: a button and the keyboard shortcuts.
  let { onnew }: { onnew: () => void } = $props();

  const SHORTCUTS: [string[], string][] = [
    [["Ctrl", "N"], "New note"],
    [["Ctrl", "Shift", "F"], "Search (#tag for tags)"],
    [["Ctrl", "Shift", "O"], "Outline"],
    [["Ctrl", "E"], "Switch view: edit / split / preview"],
    [["Ctrl", "Tab"], "Next tab (Shift: previous)"],
    [["Ctrl", "W"], "Close tab"],
    [["Ctrl", "1…9"], "Go to tab"],
    [["Ctrl", "S"], "Save now"],
    [["Ctrl", "Shift", "S"], "Scroll sync"],
    [["Ctrl", "L"], "Lock"],
    [["Ctrl", "+ / − / 0"], "Zoom in / out / reset"],
  ];
</script>

<div class="start">
  <p>Select a note or create one.</p>
  <button class="primary" onclick={onnew}>New note</button>
  <dl class="keys">
    {#each SHORTCUTS as [keys, action]}
      <dt>
        {#each keys as k, i}{#if i}<span class="plus">+</span>{/if}<kbd>{k}</kbd>{/each}
      </dt>
      <dd>{action}</dd>
    {/each}
  </dl>
</div>

<style>
  .start {
    margin: auto;
    text-align: center;
    color: var(--muted);
  }
  .keys {
    display: grid;
    grid-template-columns: auto auto;
    gap: 0.45rem 1rem;
    align-items: center;
    margin: 2rem auto 0;
    width: max-content;
    font-size: 0.85rem;
    text-align: left;
  }
  dt {
    justify-self: end;
    white-space: nowrap;
  }
  dd {
    margin: 0;
  }
  kbd {
    display: inline-block;
    min-width: 1.4em;
    padding: 0.05rem 0.4rem;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--bg-elev);
    color: var(--fg);
    font-family: var(--mono);
    font-size: 0.78rem;
    text-align: center;
  }
  .plus {
    margin: 0 0.2rem;
  }
</style>

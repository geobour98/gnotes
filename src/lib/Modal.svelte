<script lang="ts" module>
  // Promise-based prompt/confirm dialogs (native ones aren't reliable in webviews).
  type Req = {
    title: string;
    message?: string;
    input?: string; // present => prompt with a text field
    confirm: string;
    danger?: boolean;
    secret?: boolean;
    resolve: (v: string | null) => void;
  };
  let current = $state<Req | null>(null);

  export function ask(title: string, initial = "", confirm = "OK"): Promise<string | null> {
    return new Promise((resolve) => (current = { title, input: initial, confirm, resolve }));
  }
  export function askSecret(title: string, confirm = "OK"): Promise<string | null> {
    return new Promise((resolve) => (current = { title, input: "", confirm, secret: true, resolve }));
  }
  export function confirmDanger(title: string, message: string, confirm = "Delete"): Promise<boolean> {
    return new Promise((resolve) => {
      current = { title, message, confirm, danger: true, resolve: (v) => resolve(v !== null) };
    });
  }
</script>

<script lang="ts">
  let value = $state("");
  let field = $state<HTMLInputElement>();

  $effect(() => {
    if (current) {
      value = current.input ?? "";
      queueMicrotask(() => {
        field?.focus();
        field?.select();
      });
    }
  });

  function done(ok: boolean) {
    const req = current;
    current = null;
    req?.resolve(ok ? (req.input !== undefined ? value : "") : null);
  }
</script>

{#if current}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => done(false)}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <form
      class="dialog"
      onclick={(e) => e.stopPropagation()}
      onsubmit={(e) => {
        e.preventDefault();
        done(true);
      }}
      onkeydown={(e) => e.key === "Escape" && done(false)}
    >
      <h3>{current.title}</h3>
      {#if current.message}<p>{current.message}</p>{/if}
      {#if current.input !== undefined}
        {#if current.secret}
          <input bind:this={field} bind:value type="password" autocomplete="off" />
        {:else}
          <input bind:this={field} bind:value spellcheck="false" />
        {/if}
      {/if}
      <div class="row">
        <button type="button" onclick={() => done(false)}>Cancel</button>
        <button type="submit" class:danger={current.danger} class="primary">{current.confirm}</button>
      </div>
    </form>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.45);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .dialog {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1.25rem;
    width: min(28rem, calc(100vw - 2rem));
    box-shadow: 0 20px 50px rgb(0 0 0 / 0.35);
  }
  h3 {
    margin: 0 0 0.75rem;
    font-size: 1rem;
  }
  p {
    margin: 0 0 1rem;
    color: var(--muted);
  }
  input {
    width: 100%;
    margin-bottom: 1rem;
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>

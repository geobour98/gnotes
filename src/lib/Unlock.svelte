<script lang="ts">
  import { api, type Status } from "./api";

  let { status, onunlocked }: { status: Status; onunlocked: () => void } = $props();

  let path = $state("");
  let password = $state("");
  let confirm = $state("");
  let creating = $state(false);
  let busy = $state(false);
  let error = $state("");

  $effect.pre(() => {
    path = status.vault_path;
    creating = !status.vault_exists;
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = "";
    if (creating && password !== confirm) {
      error = "Passwords do not match";
      return;
    }
    busy = true;
    try {
      await (creating ? api.create(path, password) : api.unlock(path, password));
      password = confirm = "";
      onunlocked();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="screen">
  <form class="card" onsubmit={submit}>
    <div class="brand">gnotes</div>
    <p class="sub">{creating ? "Create a new encrypted vault" : "Unlock your vault"}</p>

    <label>
      Vault file
      <input bind:value={path} spellcheck="false" disabled={busy} />
    </label>
    <label>
      Master password
      <!-- svelte-ignore a11y_autofocus -->
      <input type="password" bind:value={password} autofocus disabled={busy} autocomplete="off" />
    </label>
    {#if creating}
      <label>
        Confirm password
        <input type="password" bind:value={confirm} disabled={busy} autocomplete="off" />
      </label>
      <p class="hint">
        There is no password recovery. If you forget it, the notes cannot be decrypted.
      </p>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <button class="primary" type="submit" disabled={busy || !password}>
      {busy ? "Deriving key…" : creating ? "Create vault" : "Unlock"}
    </button>
    <button type="button" class="link" onclick={() => ((creating = !creating), (error = ""))}>
      {creating ? "Open an existing vault instead" : "Create a new vault instead"}
    </button>
  </form>
</div>

<style>
  .screen {
    height: 100vh;
    display: grid;
    place-items: center;
    padding: 1rem;
  }
  .card {
    width: min(26rem, 100%);
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 2rem;
  }
  .brand {
    font-size: 1.6rem;
    font-weight: 800;
    letter-spacing: -0.02em;
    color: var(--accent);
  }
  .sub {
    margin: -0.5rem 0 0.5rem;
    color: var(--muted);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .hint {
    font-size: 0.8rem;
    color: var(--muted);
    margin: 0;
  }
  .error {
    color: var(--danger);
    margin: 0;
    font-size: 0.9rem;
  }
</style>

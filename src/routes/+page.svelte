<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, splitPath, joinPath, type Note, type NoteMeta, type Status, type Tree } from "$lib/api";
  import Unlock from "$lib/Unlock.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  import Editor from "$lib/Editor.svelte";
  import Preview from "$lib/Preview.svelte";
  import Modal, { ask, askSecret, confirmDanger } from "$lib/Modal.svelte";

  type Mode = "edit" | "split" | "preview";
  const MODES: Mode[] = ["edit", "split", "preview"];

  let status = $state<Status | null>(null);
  let tree = $state<Tree>({ folders: [], notes: [] });
  let current = $state<Note | null>(null);
  let previewSource = $state("");
  let selectedFolder = $state("");
  let saveState = $state<"saved" | "dirty" | "saving" | "error">("saved");
  let mode = $state<Mode>(loadMode());
  let toast = $state("");

  // Latest editor text; kept outside $state so typing doesn't re-render the page.
  let draft = "";
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  // The editor reads this once on mount (e.g. after switching view mode).
  const currentDraft = () => draft;

  function loadMode(): Mode {
    try {
      const m = localStorage.getItem("gnotes.mode") as Mode | null;
      return m && MODES.includes(m) ? m : "split";
    } catch {
      return "split";
    }
  }
  function setMode(m: Mode) {
    mode = m;
    try {
      localStorage.setItem("gnotes.mode", m);
    } catch {}
  }

  function notify(msg: string) {
    toast = msg;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3500);
  }
  async function attempt(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      notify(String(e));
    }
  }

  async function refreshStatus() {
    status = await api.status();
    if (status.unlocked) await loadTree();
  }
  async function loadTree() {
    tree = await api.tree();
  }

  // ---------------------------------------------------------------- saving

  function onEdit(text: string) {
    draft = text;
    saveState = "dirty";
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 600);
    clearTimeout(previewTimer);
    previewTimer = setTimeout(() => (previewSource = draft), 120);
  }

  async function flush() {
    clearTimeout(saveTimer);
    if (!current || saveState !== "dirty") return;
    const id = current.id;
    const body = draft;
    saveState = "saving";
    try {
      const updated = await api.noteSave(id, body);
      if (current?.id === id) {
        current.body = body;
        current.updated = updated;
        saveState = draft === body ? "saved" : "dirty";
        if (saveState === "dirty") saveTimer = setTimeout(flush, 600);
      }
      const meta = tree.notes.find((n) => n.id === id);
      if (meta) meta.updated = updated;
    } catch (e) {
      saveState = "error";
      notify(`Save failed: ${e}`);
    }
  }

  // ----------------------------------------------------------------- notes

  async function openNote(meta: NoteMeta) {
    if (current?.id === meta.id) return;
    await flush();
    const note = await api.noteGet(meta.id);
    current = note;
    draft = previewSource = note.body;
    saveState = "saved";
  }

  async function newNote() {
    await attempt(async () => {
      const meta = await api.noteCreate(selectedFolder);
      await loadTree();
      await openNote(meta);
      if (mode === "preview") setMode("split");
    });
  }

  async function ensureFolder(folder: string) {
    if (folder && !tree.folders.includes(folder)) await api.folderCreate(folder);
  }

  async function renameNote(n: NoteMeta) {
    const input = await ask("Rename or move note (folder/title)", joinPath(n.folder, n.title), "Save");
    if (input === null) return;
    const { folder, title } = splitPath(input);
    if (folder === n.folder && title === n.title) return;
    await attempt(async () => {
      await ensureFolder(folder);
      const meta = await api.noteMove(n.id, folder, title);
      if (current?.id === n.id) {
        current.folder = meta.folder;
        current.title = meta.title;
      }
      await loadTree();
    });
  }

  async function deleteNote(n: NoteMeta) {
    if (!(await confirmDanger(`Delete "${n.title}"?`, "This cannot be undone."))) return;
    await attempt(async () => {
      if (current?.id === n.id) {
        clearTimeout(saveTimer);
        current = null;
      }
      await api.noteDelete(n.id);
      await loadTree();
    });
  }

  // --------------------------------------------------------------- folders

  async function newFolder() {
    const input = await ask("New folder", selectedFolder ? `${selectedFolder}/` : "", "Create");
    if (!input) return;
    const path = input.trim().replace(/^\/+|\/+$/g, "");
    await attempt(async () => {
      await api.folderCreate(path);
      selectedFolder = path;
      await loadTree();
    });
  }

  async function renameFolder(path: string) {
    const input = await ask("Rename or move folder", path, "Save");
    if (input === null) return;
    const next = input.trim().replace(/^\/+|\/+$/g, "");
    if (next === path) return;
    await attempt(async () => {
      await flush();
      await api.folderRename(path, next);
      if (current && (current.folder === path || current.folder.startsWith(path + "/"))) {
        current.folder = next + current.folder.slice(path.length);
      }
      if (selectedFolder === path) selectedFolder = next;
      await loadTree();
    });
  }

  async function deleteFolder(path: string) {
    const inside = tree.notes.filter((n) => n.folder === path || n.folder.startsWith(path + "/")).length;
    const ok = await confirmDanger(
      `Delete folder "${path}"?`,
      inside ? `This also deletes ${inside} note(s) inside it. This cannot be undone.` : "The folder is empty.",
    );
    if (!ok) return;
    await attempt(async () => {
      if (current && (current.folder === path || current.folder.startsWith(path + "/"))) {
        clearTimeout(saveTimer);
        current = null;
      }
      await api.folderDelete(path);
      if (selectedFolder === path || selectedFolder.startsWith(path + "/")) selectedFolder = "";
      await loadTree();
    });
  }

  // ------------------------------------------------------- vault & settings

  async function backup() {
    await attempt(async () => {
      await flush();
      notify(`Backup saved: ${await api.backup()}`);
    });
  }

  async function lock() {
    await flush();
    await api.lock();
    onLocked();
  }

  /** Drops all decrypted content from the UI. */
  function onLocked() {
    clearTimeout(saveTimer);
    current = null;
    draft = previewSource = "";
    tree = { folders: [], notes: [] };
    saveState = "saved";
    refreshStatus();
  }

  async function settingsAutoLock() {
    const input = await ask("Auto-lock after N minutes of inactivity (0 = never)", String(status?.auto_lock_minutes ?? 10), "Save");
    if (input === null) return;
    const minutes = Number.parseInt(input, 10);
    if (!Number.isFinite(minutes) || minutes < 0) return notify("Enter a whole number of minutes");
    await attempt(async () => {
      await api.setAutoLock(minutes);
      await refreshStatus();
      notify(minutes ? `Auto-lock set to ${minutes} min` : "Auto-lock disabled");
    });
  }

  async function settingsPassword() {
    const cur = await askSecret("Current master password", "Next");
    if (cur === null) return;
    const next = await askSecret("New master password (min 8 characters)", "Next");
    if (next === null) return;
    const again = await askSecret("Repeat new master password", "Change password");
    if (again === null) return;
    if (next !== again) return notify("Passwords do not match");
    await attempt(async () => {
      await flush();
      await api.changePassword(cur, next);
      notify("Master password changed");
    });
  }

  // ------------------------------------------------------------ lifecycle

  let lastPing = 0;
  function onActivity() {
    const now = Date.now();
    if (status?.unlocked && now - lastPing > 20_000) {
      lastPing = now;
      api.activity().catch(() => {});
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (!status?.unlocked || !(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (k === "n") (e.preventDefault(), newNote());
    else if (k === "s") (e.preventDefault(), flush());
    else if (k === "l") (e.preventDefault(), lock());
    else if (k === "e") (e.preventDefault(), setMode(MODES[(MODES.indexOf(mode) + 1) % MODES.length]));
  }

  onMount(() => {
    refreshStatus();
    const unlisten = listen("vault-locked", () => {
      onLocked();
      notify("Vault locked after inactivity");
    });
    const onBlur = () => flush();
    window.addEventListener("blur", onBlur);
    return () => {
      unlisten.then((f) => f());
      window.removeEventListener("blur", onBlur);
    };
  });

  const saveLabel = { saved: "Saved", dirty: "Edited", saving: "Saving…", error: "Save failed" };
</script>

<svelte:window onkeydown={(e) => (onActivity(), onKeydown(e))} onmousedown={onActivity} />

<Modal />

{#if status && !status.unlocked}
  <Unlock {status} onunlocked={refreshStatus} />
{:else if status}
  <div class="app">
    <aside>
      <header class="side-head">
        <span class="brand">gnotes</span>
        <span class="spacer"></span>
        <button class="icon" title="New note (Ctrl+N)" onclick={newNote}>＋</button>
        <button class="icon" title="New folder" onclick={newFolder}>▤</button>
      </header>
      <Sidebar
        {tree}
        selectedId={current?.id ?? null}
        bind:selectedFolder
        onopen={(n) => attempt(() => openNote(n))}
        onrenameNote={renameNote}
        ondeleteNote={deleteNote}
        onrenameFolder={renameFolder}
        ondeleteFolder={deleteFolder}
      />
      <footer class="side-foot">
        <button onclick={backup} title="Write an encrypted backup next to the vault">Backup</button>
        <button onclick={settingsAutoLock} title="Auto-lock timeout">Auto-lock</button>
        <button onclick={settingsPassword} title="Change master password">Password</button>
        <button class="primary" onclick={lock} title="Lock (Ctrl+L)">Lock</button>
      </footer>
    </aside>

    <main>
      {#if current}
        <header class="note-head">
          <button class="crumb" title="Rename / move" onclick={() => current && renameNote(current)}>
            {#if current.folder}<span class="muted">{current.folder} /</span>{/if}
            {current.title}
          </button>
          <span class="spacer"></span>
          <span class="save {saveState}">{saveLabel[saveState]}</span>
          <div class="seg" role="group" aria-label="View mode (Ctrl+E)">
            {#each MODES as m}
              <button class:on={mode === m} onclick={() => setMode(m)}>{m}</button>
            {/each}
          </div>
        </header>
        <div class="panes {mode}">
          {#key current.id}
            {#if mode !== "preview"}
              <section class="pane edit"><Editor doc={currentDraft()} onchange={onEdit} /></section>
            {/if}
          {/key}
          {#if mode !== "edit"}
            <section class="pane preview"><Preview source={previewSource} /></section>
          {/if}
        </div>
      {:else}
        <div class="empty">
          <p>Select a note or create one.</p>
          <button class="primary" onclick={newNote}>New note</button>
          <p class="keys">Ctrl+N new · Ctrl+E switch view · Ctrl+S save · Ctrl+L lock</p>
        </div>
      {/if}
    </main>
  </div>
{/if}

{#if toast}<div class="toast" role="status">{toast}</div>{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 17rem 1fr;
    height: 100vh;
  }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-side);
    border-right: 1px solid var(--border);
  }
  aside > :global(.sidebar) {
    flex: 1;
  }
  .side-head,
  .note-head {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.5rem 0.75rem;
    min-height: 2.75rem;
    border-bottom: 1px solid var(--border);
  }
  .brand {
    font-weight: 800;
    color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  .icon {
    padding: 0.15rem 0.5rem;
  }
  .side-foot {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    padding: 0.5rem;
    border-top: 1px solid var(--border);
  }
  .side-foot button {
    flex: 1;
    font-size: 0.8rem;
    padding: 0.3rem 0.4rem;
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .crumb {
    border: none;
    background: none;
    font-weight: 600;
    padding: 0.2rem 0.3rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
    font-weight: 400;
  }
  .save {
    font-size: 0.8rem;
    color: var(--muted);
    margin-right: 0.5rem;
  }
  .save.error {
    color: var(--danger);
  }
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    border: none;
    border-radius: 0;
    padding: 0.2rem 0.6rem;
    font-size: 0.8rem;
    text-transform: capitalize;
  }
  .seg button.on {
    background: var(--selection);
  }
  .panes {
    flex: 1;
    display: grid;
    min-height: 0;
  }
  .panes.split {
    grid-template-columns: 1fr 1fr;
  }
  .pane {
    min-height: 0;
    min-width: 0;
    overflow: auto;
  }
  .panes.split .edit {
    border-right: 1px solid var(--border);
  }
  .empty {
    margin: auto;
    text-align: center;
    color: var(--muted);
  }
  .keys {
    font-size: 0.8rem;
    margin-top: 1.5rem;
  }
  .toast {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    max-width: 32rem;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0.6rem 0.9rem;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.3);
    font-size: 0.9rem;
    word-break: break-all;
  }
</style>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import {
    api,
    attachmentMarkdown,
    splitPath,
    joinPath,
    type AttachmentMeta,
    type Note,
    type NoteMeta,
    type SearchHit,
    type Status,
    type TagCount,
    type Tree,
  } from "$lib/api";
  import Unlock from "$lib/Unlock.svelte";
  import { toggleFenceWrap } from "$lib/markdown";
  import { htmlDocument, printNote, renderedCopy } from "$lib/exportNote";
  import Sidebar from "$lib/Sidebar.svelte";
  import Search from "$lib/Search.svelte";
  import Tags from "$lib/Tags.svelte";
  import Outline from "$lib/Outline.svelte";
  import Dropdown, { type MenuItem } from "$lib/Dropdown.svelte";
  import StartView from "$lib/StartView.svelte";
  import * as prefs from "$lib/prefs";
  import Editor, { type EditorSnapshot } from "$lib/Editor.svelte";
  import TabBar from "$lib/TabBar.svelte";
  import Preview from "$lib/Preview.svelte";
  import Modal, { ask, askImportTarget, askSecret, confirmDanger, showInfo } from "$lib/Modal.svelte";
  import { restoreZoom, stepZoom, zoomKey } from "$lib/zoom";

  type Mode = "edit" | "split" | "preview";
  const MODES: Mode[] = ["edit", "split", "preview"];

  let status = $state<Status | null>(null);
  let tree = $state<Tree>({ folders: [], notes: [] });
  let current = $state<Note | null>(null);
  // Open notes, like browser tabs. `current` is the note of tabs[active];
  // active is -1 on the start view.
  let tabs = $state<NoteMeta[]>([]);
  let active = $state(-1);
  // Editor state (text, cursor, undo history, scroll) per note id, so
  // switching tabs or views doesn't lose undo. Not reactive on purpose.
  const snapshots = new Map<string, EditorSnapshot>();
  let tabsRestored = false;
  let previewSource = $state("");
  let selectedFolder = $state("");
  let saveState = $state<"saved" | "dirty" | "saving" | "error">("saved");
  let mode = $state<Mode>(prefs.loadString("gnotes.mode", MODES, "split"));
  let syncScroll = $state(prefs.loadFlag("gnotes.syncScroll", true));
  let showOutline = $state(prefs.loadFlag("gnotes.outline", false));
  // Source line at the top of the visible pane (for the outline highlight).
  let viewTop = $state(0);
  let editorRef = $state<ReturnType<typeof Editor>>();
  let previewRef = $state<ReturnType<typeof Preview>>();
  let toast = $state("");
  let searching = $state(false);
  let searchQuery = $state("");
  let tagList = $state<TagCount[]>([]);
  // The tag being searched for, when the query is just one tag.
  let activeTag = $derived.by(() => {
    const m = searching ? /^\s*(?:#|tag:)([^\s#]+)\s*$/i.exec(searchQuery) : null;
    return m ? m[1].toLowerCase().split("/").filter(Boolean).join("/") : null;
  });
  let tagsTimer: ReturnType<typeof setTimeout> | undefined;
  let searchRef = $state<ReturnType<typeof Search>>();
  // Line the preview should show once the note it belongs to has rendered.
  let pendingPreviewLine: number | null = null;

  // Latest editor text; kept outside $state so typing doesn't re-render the page.
  let draft = "";
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  // The editor reads this once on mount (e.g. after switching view mode).
  const currentDraft = () => draft;

  function setMode(m: Mode) {
    mode = m;
    prefs.save("gnotes.mode", m);
    if (m === "split") requestAnimationFrame(() => syncFrom("editor"));
  }

  // ---------------------------------------------------------- scroll sync
  // Whichever pane the user scrolls "drives" for a short window; scroll
  // events from the pane we moved programmatically are ignored, which
  // prevents the two panes from fighting each other.

  // "jump" = the top/bottom buttons moved both panes; sync stays out of it.
  let driver: "editor" | "preview" | "jump" | null = null;
  let driverTimer: ReturnType<typeof setTimeout> | undefined;

  function toggleSync() {
    syncScroll = !syncScroll;
    prefs.save("gnotes.syncScroll", syncScroll);
    if (syncScroll) syncFrom("editor");
    notify(syncScroll ? "Scroll sync on" : "Scroll sync off");
  }

  function toggleOutline() {
    showOutline = !showOutline;
    prefs.save("gnotes.outline", showOutline);
  }

  /** Outline click: put the heading at the top of both panes. */
  function jumpToLine(line: number) {
    driver = "jump";
    clearTimeout(driverTimer);
    driverTimer = setTimeout(() => (driver = null), 250);
    editorRef?.scrollToLine(line);
    previewRef?.scrollToLine(line);
    viewTop = line;
  }

  /** Top/bottom buttons: move both panes to the edge, with sync paused. */
  function jumpTo(edge: "top" | "bottom") {
    driver = "jump";
    clearTimeout(driverTimer);
    driverTimer = setTimeout(() => (driver = null), 250);
    editorRef?.scrollToEdge(edge);
    previewRef?.scrollToEdge(edge);
  }

  function syncFrom(source: "editor" | "preview", line?: number) {
    if (!syncScroll || mode !== "split" || !editorRef || !previewRef) return;
    if (driver && driver !== source) return;
    driver = source;
    clearTimeout(driverTimer);
    driverTimer = setTimeout(() => (driver = null), 150);
    if (source === "editor") previewRef.scrollToLine(line ?? editorRef.topLine());
    else editorRef.scrollToLine(line ?? previewRef.topLine());
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
    await syncTabs();
    await loadTags();
  }
  async function loadTags() {
    clearTimeout(tagsTimer);
    try {
      tagList = await api.tags();
    } catch {}
  }

  /** Shows the notes with a tag (clicked in the sidebar or the preview). */
  function showTag(tag: string) {
    searching = true;
    searchRef?.setQuery(`#${tag}`);
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

  let inflight: Promise<void> | null = null;

  /** Saves pending edits now; waits for a save that is already running. */
  async function flush() {
    clearTimeout(saveTimer);
    while (inflight) await inflight;
    if (!current || saveState !== "dirty") return;
    inflight = save();
    try {
      await inflight;
    } finally {
      inflight = null;
    }
  }

  async function save() {
    if (!current) return;
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
      clearTimeout(tagsTimer);
      tagsTimer = setTimeout(loadTags, 1500);
    } catch (e) {
      saveState = "error";
      notify(`Save failed: ${e}`);
    }
  }

  // ------------------------------------------------------------------ tabs

  /**
   * Opens a note: in the current tab (replacing it), or in a new tab when
   * `newTab` is set. A note that is already open just gets its tab focused.
   */
  async function openNote(meta: NoteMeta, newTab = false) {
    const open = tabs.findIndex((t) => t.id === meta.id);
    if (open >= 0) return activateTab(open);
    await flush();
    const entry = { id: meta.id, folder: meta.folder, title: meta.title, updated: meta.updated };
    if (newTab || active < 0) {
      tabs.push(entry);
      await loadTab(tabs.length - 1);
    } else {
      snapshots.delete(tabs[active].id);
      tabs[active] = entry;
      await loadTab(active);
    }
  }

  async function activateTab(i: number) {
    if (i === active && current) return;
    await flush();
    await loadTab(i);
  }

  /** Shows tabs[i] (pending edits must already be saved). */
  async function loadTab(i: number) {
    const note = await api.noteGet(tabs[i].id);
    active = i;
    current = note;
    draft = previewSource = note.body;
    viewTop = 0;
    saveState = "saved";
    saveTabs();
  }

  async function closeTab(i: number) {
    if (i < 0 || i >= tabs.length) return;
    if (i === active) await flush();
    const [closed] = tabs.splice(i, 1);
    snapshots.delete(closed.id);
    if (i === active) {
      clearTimeout(saveTimer);
      current = null;
      active = -1;
      if (tabs.length) await loadTab(Math.min(i, tabs.length - 1));
      else showStart();
    } else if (i < active) {
      active--;
    }
    saveTabs();
  }

  function cycleTab(step: number) {
    if (!tabs.length) return;
    const from = active < 0 ? (step > 0 ? -1 : 0) : active;
    attempt(() => activateTab((from + step + tabs.length) % tabs.length));
  }

  /** Clears the editor area (start view); tabs stay open. */
  function showStart() {
    clearTimeout(saveTimer);
    current = null;
    active = -1;
    draft = previewSource = "";
    saveState = "saved";
    viewTop = 0;
  }

  /** The editor's saved state for a note, if it still matches its text. */
  function snapshotFor(id: string, text: string): EditorSnapshot | undefined {
    const snap = snapshots.get(id);
    if (snap && snap.state.doc.toString() === text) return snap;
    snapshots.delete(id);
    return undefined;
  }

  /**
   * After the tree changed: drop tabs of deleted notes, refresh renamed or
   * moved ones, and restore the tabs saved in the vault after an unlock.
   */
  async function syncTabs() {
    const byId = new Map(tree.notes.map((n) => [n.id, n]));
    if (!tabsRestored) {
      tabsRestored = true;
      await restoreTabs(byId);
      return;
    }
    const activeId = active >= 0 ? tabs[active]?.id : undefined;
    const next = tabs.filter((t) => byId.has(t.id)).map((t) => ({ ...byId.get(t.id)! }));
    for (const t of tabs) if (!byId.has(t.id)) snapshots.delete(t.id);
    const changed =
      next.length !== tabs.length ||
      next.some((t, i) => t.title !== tabs[i].title || t.folder !== tabs[i].folder);
    if (!changed) return;
    const oldActive = active;
    tabs = next;
    if (activeId && !byId.has(activeId)) {
      // The open note was deleted: show a neighbour.
      showStart();
      if (tabs.length) await loadTab(Math.min(Math.max(oldActive, 0), tabs.length - 1));
    } else {
      active = activeId ? tabs.findIndex((t) => t.id === activeId) : -1;
    }
    saveTabs();
  }

  // The open tabs are stored encrypted inside the vault ("ui.tabs"), never
  // in the plain config file: note ids would reveal what the vault holds.
  type SavedTabs = { ids: string[]; active: number };

  function saveTabs() {
    const value: SavedTabs = { ids: tabs.map((t) => t.id), active };
    api.uiSet("ui.tabs", JSON.stringify(value)).catch(() => {});
  }

  async function restoreTabs(byId: Map<string, NoteMeta>) {
    let saved: SavedTabs | null = null;
    try {
      saved = JSON.parse((await api.uiGet("ui.tabs")) ?? "null");
    } catch {}
    if (!saved || !Array.isArray(saved.ids)) return;
    const wanted = saved.ids.filter((id) => typeof id === "string" && byId.has(id));
    tabs = wanted.map((id) => ({ ...byId.get(id)! }));
    const activeId = saved.ids[saved.active];
    const i = tabs.findIndex((t) => t.id === activeId);
    if (i >= 0) await loadTab(i).catch(() => showStart());
    else if (tabs.length) active = -1;
  }

  // ----------------------------------------------------------------- notes

  /** Opens a search result, scrolled to (and selecting) the matching line. */
  async function openSearchHit(hit: SearchHit, line: number | null, terms: string[]) {
    await attempt(async () => {
      const same = current?.id === hit.id;
      if (!same) pendingPreviewLine = line;
      await openNote(hit);
      if (line === null) return;
      await tick();
      requestAnimationFrame(() => {
        editorRef?.revealLine(line, terms);
        if (same) previewRef?.scrollToLine(line);
      });
    });
  }

  function onPreviewRendered() {
    if (pendingPreviewLine !== null && previewRef) {
      const line = pendingPreviewLine;
      pendingPreviewLine = null;
      // Hold off the editor→preview sync while the preview jumps.
      driver = "jump";
      clearTimeout(driverTimer);
      driverTimer = setTimeout(() => (driver = null), 250);
      previewRef.scrollToLine(line);
    } else syncFrom("editor");
  }

  function toggleSearch(on = !searching) {
    searching = on;
    if (on) requestAnimationFrame(() => searchRef?.focus());
  }

  /** Closes the open note (after saving) and shows the start view. */
  async function goHome() {
    await attempt(async () => {
      await flush();
      showStart();
      saveTabs();
    });
  }

  async function newNote() {
    await attempt(async () => {
      const meta = await api.noteCreate(selectedFolder);
      await loadTree();
      await openNote(meta, true);
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
      if (current?.id === n.id) clearTimeout(saveTimer); // its tab closes after the reload
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
      inside
        ? `This also deletes ${inside} note(s) inside it. This cannot be undone.`
        : "The folder is empty.",
    );
    if (!ok) return;
    await attempt(async () => {
      if (current && (current.folder === path || current.folder.startsWith(path + "/"))) {
        clearTimeout(saveTimer); // tabs of deleted notes close after the reload
      }
      await api.folderDelete(path);
      if (selectedFolder === path || selectedFolder.startsWith(path + "/")) selectedFolder = "";
      await loadTree();
    });
  }

  /** Wrap button on a code block: toggles `wrap` on its fence line. */
  function toggleCodeWrap(line: number) {
    const lines = draft.split("\n");
    const next = toggleFenceWrap(lines[line] ?? "");
    if (next === null) return;
    if (editorRef) {
      editorRef.replaceLine(line, next); // goes through onEdit (save + preview)
    } else {
      lines[line] = next;
      onEdit(lines.join("\n"));
    }
  }

  // ----------------------------------------------------------- attachments

  function insertAttachments(metas: AttachmentMeta[]) {
    if (!metas.length) return;
    editorRef?.insertText(metas.map(attachmentMarkdown).join("\n") + "\n");
    notify(metas.length === 1 ? `Attached ${metas[0].name}` : `Attached ${metas.length} files`);
  }

  /** Attachments go in at the editor cursor, so the editor must be visible. */
  function canAttach(): boolean {
    if (!current) {
      notify("Open a note first to attach files");
      return false;
    }
    if (mode === "preview") setMode("split");
    return true;
  }

  async function attachPicked() {
    if (!canAttach()) return;
    const picked = await openDialog({ multiple: true, title: "Attach files" });
    if (!picked) return;
    await attempt(async () => insertAttachments(await api.attachmentAddPaths([picked].flat())));
  }

  async function attachPasted(files: File[]) {
    await attempt(async () => {
      const metas: AttachmentMeta[] = [];
      for (const f of files) {
        // Screenshots arrive as a generic "image.png"; give them a useful name.
        let name = f.name || "pasted";
        if (/^image\.\w+$/.test(name)) {
          const stamp = new Date().toISOString().slice(0, 19).replace(/[-:]/g, "").replace("T", "-");
          name = `Pasted image ${stamp}.${name.split(".").pop()}`;
        }
        metas.push(await api.attachmentAdd(name, f.type, new Uint8Array(await f.arrayBuffer())));
      }
      insertAttachments(metas);
    });
  }

  async function attachDropped(paths: string[]) {
    if (!paths.length || !status?.unlocked || !canAttach()) return;
    // Let the editor mount if we just switched out of preview mode.
    await new Promise(requestAnimationFrame);
    await attempt(async () => insertAttachments(await api.attachmentAddPaths(paths)));
  }

  /** Clicking an attachment link saves a decrypted copy where the user chooses. */
  async function saveAttachment(id: string) {
    await attempt(async () => {
      const meta = await api.attachmentMeta(id);
      const path = await saveDialog({ title: "Save a decrypted copy", defaultPath: meta.name });
      if (!path) return;
      await api.attachmentSave(id, path);
      notify(`Saved ${path}`);
    });
  }

  // ------------------------------------------------------- single-note export

  // Filled only while printing (see printNote).
  let printRoot = $state<HTMLDivElement>();

  /** The note as rendered: from the open preview, or rendered off-screen. */
  async function renderedNote() {
    await flush();
    previewSource = draft;
    await tick();
    return renderedCopy(mode !== "edit" ? previewRef?.getArticle() : undefined, draft);
  }

  async function exportNoteHtml() {
    const note = current;
    if (!note) return;
    const path = await saveDialog({
      title: "Export note as HTML",
      defaultPath: `${note.title}.html`,
      filters: [{ name: "HTML", extensions: ["html"] }],
    });
    if (!path) return;
    await attempt(async () => {
      const n = await api.exportNoteHtml(htmlDocument(note.title, await renderedNote()), path);
      notify(`Exported to ${path}${n ? ` with ${n} image${n === 1 ? "" : "s"} embedded` : ""}`);
    });
  }

  async function exportNoteMarkdown() {
    const note = current;
    if (!note) return;
    const dir = await openDialog({ directory: true, title: `Export "${note.title}" as Markdown into…` });
    if (!dir || Array.isArray(dir)) return;
    await attempt(async () => {
      await flush();
      const r = await api.exportNoteMarkdown(note.id, dir);
      notify(
        `Exported to ${r.path}${r.attachments ? ` (+${r.attachments} attachment${r.attachments === 1 ? "" : "s"})` : ""}`,
      );
    });
  }

  async function printCurrentNote() {
    if (!current || !printRoot) return;
    const root = printRoot;
    await attempt(async () => printNote(root, await renderedNote()));
  }

  // ---------------------------------------------- vault import / export

  async function importFolder() {
    const dir = await openDialog({ directory: true, title: "Import a folder of Markdown files" });
    if (!dir || Array.isArray(dir)) return;
    const sourceName = dir.split(/[\\/]/).filter(Boolean).pop() ?? "Imported";
    const target = await askImportTarget(sourceName, tree.folders, selectedFolder);
    if (!target) return;
    await attempt(async () => {
      notify("Importing…");
      await flushAll();
      const r = await api.importFolder(dir, target.into, target.newFolder);
      await loadTree();
      selectedFolder = r.folder;
      const where = r.folder ? `“${r.folder}”` : "the top level";
      const msg = `Imported ${r.notes} notes and ${r.attachments} attachments into ${where}.`;
      if (r.warnings.length) {
        const shown = r.warnings.slice(0, 200);
        const more =
          r.warnings.length > shown.length ? `\n… and ${r.warnings.length - shown.length} more` : "";
        await showInfo(
          "Import finished with warnings",
          `${msg}\n\nThese referenced files were not imported:\n• ${shown.join("\n• ")}${more}`,
        );
      } else {
        notify(msg);
      }
    });
  }

  /** Lists problems in a dialog (capped so huge lists stay readable). */
  function warningList(warnings: string[]): string {
    const shown = warnings.slice(0, 200);
    const more = warnings.length > shown.length ? `\n… and ${warnings.length - shown.length} more` : "";
    return `• ${shown.join("\n• ")}${more}`;
  }

  async function linkImagesFromFolder() {
    const dir = await openDialog({
      directory: true,
      title: "Choose the folder your notes take images from (e.g. img)",
    });
    if (!dir || Array.isArray(dir)) return;
    await attempt(async () => {
      notify("Importing images…");
      await flushAll();
      const r = await api.linkFilesFromFolder(dir);
      await loadTree();
      // The open note may have been rewritten: reload it so the editor
      // (and autosave) work on the repaired text.
      if (current && active >= 0) {
        const i = active;
        current = null;
        await tick();
        await loadTab(i);
      }
      const msg = r.links
        ? `Copied ${r.attachments} images into the vault and updated ${r.links} references in ${r.notes} notes.`
        : "No note references a file in that folder.";
      if (r.warnings.length) {
        await showInfo(
          "Images imported: some files are still missing",
          `${msg}\n\nNot found in that folder:\n${warningList(r.warnings)}`,
        );
      } else {
        notify(msg);
      }
    });
  }

  async function exportMarkdown() {
    const ok = await confirmDanger(
      "Export an unencrypted copy?",
      "All notes and attachments will be written as plain files. Anyone who can access that folder can read them.",
      "Choose folder…",
    );
    if (!ok) return;
    const dir = await openDialog({ directory: true, title: "Choose where to create the export folder" });
    if (!dir || Array.isArray(dir)) return;
    await attempt(async () => {
      await flushAll();
      const r = await api.exportMarkdown(dir);
      await showInfo(
        "Export finished",
        `${r.notes} notes and ${r.attachments} attachments written to:\n${r.dir}\n\nThese files are NOT encrypted. Delete them when you no longer need them.`,
      );
    });
  }

  // ------------------------------------------------------- vault & settings

  async function backup() {
    await attempt(async () => {
      await flush();
      notify(`Backup saved: ${await api.backup()}`);
    });
  }

  /** Saves until nothing is pending (typing may continue during a save). */
  async function flushAll() {
    do await flush();
    while (saveState === "dirty");
  }

  /** Window close button: save, then let the backend lock and exit. */
  async function onCloseRequested() {
    await api.quitAck();
    await flushAll();
    if (saveState === "error") {
      const quit = await confirmDanger(
        "Your latest changes could not be saved",
        "If you quit now, they will be lost.",
        "Quit anyway",
      );
      if (!quit) return;
    }
    await api.quit();
  }

  async function lock() {
    await flushAll();
    await api.lock();
    onLocked();
  }

  /** Drops all decrypted content from the UI. */
  function onLocked() {
    clearTimeout(saveTimer);
    current = null;
    tabs = [];
    active = -1;
    snapshots.clear();
    tabsRestored = false;
    draft = previewSource = "";
    tree = { folders: [], notes: [] };
    tagList = [];
    clearTimeout(tagsTimer);
    searching = false;
    searchQuery = "";
    saveState = "saved";
    refreshStatus();
  }

  const LOCK_REASONS: Record<string, string> = {
    inactivity: "Vault locked after inactivity",
    screen: "Vault locked because the screen locked",
    sleep: "Vault locked before sleep",
  };

  /** Screen lock / sleep: save pending edits, then lock (the backend locks
   *  on its own if this takes too long). */
  async function onLockRequested(reason: string) {
    try {
      await flushAll();
    } catch {}
    try {
      await api.lock();
    } catch {}
    onLocked();
    notify(LOCK_REASONS[reason] ?? "Vault locked");
  }

  async function toggleLockOnScreenLock() {
    const enabled = !status?.lock_on_screen_lock;
    await attempt(async () => {
      await api.setLockOnScreenLock(enabled);
      await refreshStatus();
      notify(
        enabled ? "Will lock when the screen locks or the computer sleeps" : "Screen-lock locking disabled",
      );
    });
  }

  async function settingsAutoLock() {
    const input = await ask(
      "Auto-lock after N minutes of inactivity (0 = never)",
      String(status?.auto_lock_minutes ?? 10),
      "Save",
    );
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

  async function zoom(direction: -1 | 0 | 1) {
    try {
      notify(`Zoom ${Math.round((await stepZoom(direction)) * 100)}%`);
    } catch (e) {
      notify(`Zoom failed: ${e}`);
    }
  }

  // Ctrl+wheel zooms; it must be non-passive to stop the page scrolling.
  // Wheel deltas arrive in bursts (touchpads), so step at most every 120 ms.
  let lastWheelZoom = 0;
  function onWheel(e: WheelEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.deltaY === 0) return;
    e.preventDefault();
    const now = Date.now();
    if (now - lastWheelZoom < 120) return;
    lastWheelZoom = now;
    zoom(e.deltaY < 0 ? 1 : -1);
  }

  function onKeydown(e: KeyboardEvent) {
    // Zoom works everywhere, including the unlock screen.
    const z = zoomKey(e);
    if (z !== null) {
      e.preventDefault();
      zoom(z);
      return;
    }
    if (!status?.unlocked || !(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (k === "tab") {
      e.preventDefault();
      cycleTab(e.shiftKey ? -1 : 1);
      return;
    }
    if (!e.shiftKey && /^[1-9]$/.test(k) && tabs.length) {
      // Ctrl+1…8: that tab; Ctrl+9: the last one (as in browsers).
      e.preventDefault();
      const i = k === "9" ? tabs.length - 1 : Number(k) - 1;
      if (i < tabs.length) attempt(() => activateTab(i));
      return;
    }
    if (e.shiftKey) {
      if (k === "s") (e.preventDefault(), toggleSync());
      else if (k === "f") (e.preventDefault(), toggleSearch(true));
      else if (k === "o") (e.preventDefault(), toggleOutline());
      return;
    }
    if (k === "n") (e.preventDefault(), newNote());
    else if (k === "s") (e.preventDefault(), flush());
    else if (k === "l") (e.preventDefault(), lock());
    else if (k === "w") (e.preventDefault(), active >= 0 && attempt(() => closeTab(active)));
    else if (k === "e") (e.preventDefault(), setMode(MODES[(MODES.indexOf(mode) + 1) % MODES.length]));
  }

  onMount(() => {
    refreshStatus();
    restoreZoom().catch(() => {});
    window.addEventListener("wheel", onWheel, { passive: false });
    const unlisten = listen<string>("vault-locked", (e) => {
      onLocked();
      notify(LOCK_REASONS[e.payload] ?? "Vault locked");
    });
    const unlistenLockReq = listen<string>("lock-requested", (e) => {
      if (status?.unlocked) onLockRequested(e.payload);
    });
    const unlistenDrop = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop") attachDropped(e.payload.paths);
    });
    const unlistenClose = listen("close-requested", () => {
      onCloseRequested().catch(() => api.quit());
    });
    const onBlur = () => flush();
    window.addEventListener("blur", onBlur);
    return () => {
      unlisten.then((f) => f());
      unlistenLockReq.then((f) => f());
      unlistenClose.then((f) => f());
      unlistenDrop.then((f) => f());
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("wheel", onWheel);
    };
  });

  const exportMenu: MenuItem[] = [
    { label: "HTML file (images included)…", run: exportNoteHtml },
    { label: "Markdown with attachments…", run: exportNoteMarkdown },
    { label: "Print / Save as PDF…", run: printCurrentNote },
  ];

  let vaultMenu = $derived<MenuItem[]>([
    { label: "Import Markdown folder…", run: importFolder },
    { label: "Import images from folder…", run: linkImagesFromFolder },
    { label: "Export as Markdown…", run: exportMarkdown },
    "-",
    { label: "Backup now", run: backup },
    { label: "Auto-lock…", run: settingsAutoLock },
    {
      label: "Lock on screen lock / sleep",
      run: toggleLockOnScreenLock,
      checked: status?.lock_on_screen_lock ?? true,
    },
    { label: "Change password…", run: settingsPassword },
  ]);

  const saveLabel = { saved: "Saved", dirty: "Edited", saving: "Saving…", error: "Save failed" };
</script>

<svelte:window
  onkeydown={(e) => {
    onActivity();
    onKeydown(e);
  }}
  onmousedown={onActivity}
/>

<Modal />

{#if status && !status.unlocked}
  <Unlock {status} onunlocked={refreshStatus} />
{:else if status}
  <div class="app">
    <aside>
      <header class="side-head">
        <button class="brand" title="Close the note (home)" onclick={goHome}>gnotes</button>
        <span class="spacer"></span>
        <button
          class="icon"
          class:on={searching}
          aria-pressed={searching}
          title="Search notes (Ctrl+Shift+F)"
          onclick={() => toggleSearch()}>⌕</button
        >
        <button class="icon" title="New note (Ctrl+N)" onclick={newNote}>＋</button>
        <button class="icon" title="New folder" onclick={newFolder}>▤</button>
      </header>
      <Search
        bind:this={searchRef}
        bind:query={searchQuery}
        visible={searching}
        selectedId={current?.id ?? null}
        onopen={openSearchHit}
        onclose={() => toggleSearch(false)}
      />
      <Sidebar
        hidden={searching}
        {tree}
        selectedId={current?.id ?? null}
        bind:selectedFolder
        onopen={(n, newTab) => attempt(() => openNote(n, newTab))}
        onrenameNote={renameNote}
        ondeleteNote={deleteNote}
        onrenameFolder={renameFolder}
        ondeleteFolder={deleteFolder}
      />
      <Tags tags={tagList} {activeTag} ontag={showTag} />
      <footer class="side-foot">
        <Dropdown label="Vault ▴" items={vaultMenu} up variant="block" />
        <button class="primary" onclick={lock} title="Lock (Ctrl+L)">Lock</button>
      </footer>
    </aside>

    <main>
      {#if tabs.length}
        <TabBar
          {tabs}
          {active}
          dirty={saveState === "dirty" || saveState === "saving" || saveState === "error"}
          onselect={(i) => attempt(() => activateTab(i))}
          onclose={(i) => attempt(() => closeTab(i))}
        />
      {/if}
      {#if current}
        <header class="note-head">
          <button class="crumb" title="Rename / move" onclick={() => current && renameNote(current)}>
            {#if current.folder}<span class="muted">{current.folder} /</span>{/if}
            {current.title}
          </button>
          <span class="spacer"></span>
          <span class="save {saveState}">{saveLabel[saveState]}</span>
          <button class="tool" title="Attach files (or paste / drop them)" onclick={attachPicked}
            >📎 Attach</button
          >
          <Dropdown label="⇪ Export ▾" title="Export or print this note" items={exportMenu} />
          {#if mode === "split"}
            <button
              class="sync"
              class:on={syncScroll}
              aria-pressed={syncScroll}
              title="Synchronize scrolling between editor and preview (Ctrl+Shift+S)"
              onclick={toggleSync}>⇅ Sync</button
            >
          {/if}
          <button
            class="sync"
            class:on={showOutline}
            aria-pressed={showOutline}
            title="Show the note's headings (Ctrl+Shift+O)"
            onclick={toggleOutline}>☰ Outline</button
          >
          <div class="seg" role="group" aria-label="View mode (Ctrl+E)">
            {#each MODES as m}
              <button class:on={mode === m} onclick={() => setMode(m)}>{m}</button>
            {/each}
          </div>
        </header>
        <div class="body">
          <div class="panes {mode}">
            <div class="jump" role="group" aria-label="Jump">
              <button title="Go to top" aria-label="Go to top" onclick={() => jumpTo("top")}>↑</button>
              <button title="Go to bottom" aria-label="Go to bottom" onclick={() => jumpTo("bottom")}
                >↓</button
              >
            </div>
            {#key current.id}
              {#if mode !== "preview"}
                {@const tabId = current.id}
                <section class="pane edit">
                  <Editor
                    bind:this={editorRef}
                    doc={currentDraft()}
                    initial={snapshotFor(tabId, currentDraft())}
                    onsnapshot={(snap) => {
                      // Only for tabs still open: a closed tab's text shouldn't linger.
                      if (tabs.some((t) => t.id === tabId)) snapshots.set(tabId, snap);
                    }}
                    onchange={onEdit}
                    onscrollline={(l) => {
                      viewTop = l;
                      syncFrom("editor", l);
                    }}
                    onpastefiles={attachPasted}
                  />
                </section>
              {/if}
            {/key}
            {#if mode !== "edit"}
              <section class="pane preview">
                <Preview
                  bind:this={previewRef}
                  source={previewSource}
                  onscrollline={(l) => {
                    if (mode === "preview" || driver !== "editor") viewTop = l;
                    syncFrom("preview", l);
                  }}
                  onrendered={onPreviewRendered}
                  onattachment={saveAttachment}
                  ontag={showTag}
                  ontogglewrap={toggleCodeWrap}
                />
              </section>
            {/if}
          </div>
          {#if showOutline}
            <Outline source={previewSource} topLine={viewTop} onjump={jumpToLine} onclose={toggleOutline} />
          {/if}
        </div>
      {:else}
        <StartView onnew={newNote} />
      {/if}
    </main>
  </div>
{/if}

{#if toast}<div class="toast" role="status">{toast}</div>{/if}

<!-- Filled only while printing a note (see printNote). -->
<div class="print-root markdown" bind:this={printRoot}></div>

<style>
  .app {
    display: grid;
    grid-template-columns: 17rem 1fr;
    /* One row exactly the window's height: long sidebar content (notes,
       tags) must scroll inside its panel, never stretch the page. */
    grid-template-rows: minmax(0, 1fr);
    height: 100vh;
    overflow: hidden;
  }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-side);
    border-right: 1px solid var(--border);
  }
  aside > :global(.sidebar) {
    flex: 1 1 0;
    min-height: 0;
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
    border: none;
    background: none;
    padding: 0.1rem 0.25rem;
    font-size: inherit;
    border-radius: 6px;
    cursor: pointer;
  }
  .brand:hover {
    background: var(--hover);
  }
  .spacer {
    flex: 1;
  }
  .icon {
    padding: 0.15rem 0.5rem;
  }
  .icon.on {
    background: var(--selection);
  }
  .side-foot {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    padding: 0.5rem;
    border-top: 1px solid var(--border);
  }
  /* Vault menu and Lock button share the footer evenly. */
  .side-foot > :global(*) {
    flex: 1;
  }
  .side-foot > button {
    width: 100%;
    font-size: 0.85rem;
    padding: 0.35rem 0.5rem;
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
  .jump {
    position: absolute;
    right: 1.1rem;
    bottom: 1.1rem;
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    opacity: 0.55;
    transition: opacity 0.15s;
  }
  .jump:hover,
  .jump:focus-within {
    opacity: 1;
  }
  .jump button {
    width: 2.1rem;
    height: 2.1rem;
    padding: 0;
    border-radius: 50%;
    font-size: 1rem;
    line-height: 1;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.15);
  }
  /* Editor/preview panes plus the optional outline on the right. */
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .panes {
    position: relative;
    flex: 1;
    display: grid;
    min-height: 0;
    min-width: 0;
  }
  .panes.split {
    grid-template-columns: 1fr 1fr;
  }
  .pane {
    min-height: 0;
    min-width: 0;
    overflow: auto;
  }
  .pane.preview {
    overflow: hidden;
  }
  .tool {
    font-size: 0.8rem;
    padding: 0.2rem 0.55rem;
    margin-right: 0.35rem;
  }
  .sync {
    font-size: 0.8rem;
    padding: 0.2rem 0.55rem;
    margin-right: 0.35rem;
    color: var(--muted);
  }
  .sync.on {
    background: var(--selection);
    color: var(--fg);
  }
  .panes.split .edit {
    border-right: 1px solid var(--border);
  }
  .toast {
    position: fixed;
    bottom: 1rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 30;
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

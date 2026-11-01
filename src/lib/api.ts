// Typed wrappers around the Rust commands. This is the only way the UI
// reaches the vault.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export interface NoteMeta {
  id: string;
  folder: string;
  title: string;
  updated: number;
}
export interface Note extends NoteMeta {
  body: string;
  created: number;
}
export interface Tree {
  folders: string[];
  notes: NoteMeta[];
}
export interface AttachmentMeta {
  id: string;
  name: string;
  mime: string;
  size: number;
  created: number;
}
export interface ExportReport {
  dir: string;
  notes: number;
  attachments: number;
}
export interface ImportReport {
  folder: string;
  notes: number;
  attachments: number;
  warnings: string[];
}
export interface LinkReport {
  notes: number;
  attachments: number;
  links: number;
  warnings: string[];
}
export interface Status {
  unlocked: boolean;
  vault_path: string;
  vault_exists: boolean;
  auto_lock_minutes: number;
}

export const api = {
  status: () => invoke<Status>("vault_status"),
  create: (path: string, password: string) => invoke<void>("vault_create", { path, password }),
  unlock: (path: string, password: string) => invoke<void>("vault_unlock", { path, password }),
  lock: () => invoke<void>("vault_lock"),
  quitAck: () => invoke<void>("quit_ack"),
  quit: () => invoke<void>("app_quit"),
  activity: () => invoke<void>("activity"),
  setAutoLock: (minutes: number) => invoke<void>("set_auto_lock", { minutes }),
  changePassword: (current: string, next: string) =>
    invoke<void>("change_password", { current, new: next }),
  backup: () => invoke<string>("backup_now"),

  exportMarkdown: (parent: string) => invoke<ExportReport>("export_markdown", { parent }),
  importFolder: (path: string) => invoke<ImportReport>("import_folder", { path }),
  linkFilesFromFolder: (path: string) => invoke<LinkReport>("link_files_from_folder", { path }),

  attachmentAdd: (name: string, mime: string, bytes: Uint8Array) =>
    invoke<AttachmentMeta>("attachment_add", bytes, {
      headers: { "x-name": encodeURIComponent(name), "x-mime": encodeURIComponent(mime) },
    }),
  attachmentAddPaths: (paths: string[]) => invoke<AttachmentMeta[]>("attachment_add_paths", { paths }),
  attachmentMeta: (id: string) => invoke<AttachmentMeta>("attachment_meta", { id }),
  attachmentSave: (id: string, path: string) => invoke<void>("attachment_save", { id, path }),

  tree: () => invoke<Tree>("tree"),
  folderCreate: (path: string) => invoke<void>("folder_create", { path }),
  folderRename: (old: string, next: string) => invoke<void>("folder_rename", { old, new: next }),
  folderDelete: (path: string) => invoke<number>("folder_delete", { path }),
  noteCreate: (folder: string, title?: string) =>
    invoke<NoteMeta>("note_create", { folder, title: title ?? null }),
  noteGet: (id: string) => invoke<Note>("note_get", { id }),
  noteSave: (id: string, body: string) => invoke<number>("note_save", { id, body }),
  noteMove: (id: string, folder: string, title: string) =>
    invoke<NoteMeta>("note_move", { id, folder, title }),
  noteDelete: (id: string) => invoke<void>("note_delete", { id }),
};

/** URL the preview loads an attachment from (served from the unlocked vault). */
export function attachmentUrl(id: string): string {
  return convertFileSrc(id, "gnotes");
}

export const ATTACHMENT_ID = /^attachment:([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/;

/** Markdown that references an attachment. */
export function attachmentMarkdown(m: AttachmentMeta): string {
  const label = m.name.replace(/[[\]\\]/g, "\\$&");
  return m.mime.startsWith("image/")
    ? `![${label}](attachment:${m.id})`
    : `[📎 ${label}](attachment:${m.id})`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Splits "a/b/Title" into folder "a/b" and title "Title". */
export function splitPath(path: string): { folder: string; title: string } {
  const clean = path.trim().replace(/^\/+|\/+$/g, "");
  const i = clean.lastIndexOf("/");
  return i < 0
    ? { folder: "", title: clean }
    : { folder: clean.slice(0, i), title: clean.slice(i + 1) };
}

export function joinPath(folder: string, name: string): string {
  return folder ? `${folder}/${name}` : name;
}

// Whole-window zoom (Ctrl +/-/0, Ctrl+wheel), remembered between sessions.
import { getCurrentWebview } from "@tauri-apps/api/webview";

const LEVELS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3];
const KEY = "gnotes.zoom";

let level = load();

function load(): number {
  try {
    const v = Number(localStorage.getItem(KEY));
    return LEVELS.includes(v) ? v : 1;
  } catch {
    return 1;
  }
}

async function apply(next: number): Promise<number> {
  level = next;
  try {
    localStorage.setItem(KEY, String(level));
  } catch {}
  await getCurrentWebview().setZoom(level);
  return level;
}

/** Re-applies the saved zoom (call once at startup). */
export function restoreZoom(): Promise<number> {
  return apply(level);
}

/** Steps zoom by +1/-1 level, or resets with 0. Returns the new factor. */
export function stepZoom(direction: -1 | 0 | 1): Promise<number> {
  if (direction === 0) return apply(1);
  const i = LEVELS.indexOf(level);
  const j = Math.min(Math.max(i + direction, 0), LEVELS.length - 1);
  return apply(LEVELS[j]);
}

/** Maps a keyboard event to a zoom direction, or null if it isn't a zoom key. */
export function zoomKey(e: KeyboardEvent): -1 | 0 | 1 | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
  if (e.key === "=" || e.key === "+") return 1;
  if (e.key === "-" || e.key === "_") return -1;
  if (e.key === "0" && !e.shiftKey) return 0;
  return null;
}

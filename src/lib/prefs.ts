// Small per-machine UI preferences (view mode, toggles) in localStorage.
// Nothing about notes goes here: that would be readable without the
// password. Note-related state (e.g. open tabs) is stored in the vault.
//
// Storage can be unavailable (private mode, blocked site data), so every
// access falls back quietly.

export function loadString<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key) as T | null;
    return v !== null && allowed.includes(v) ? v : fallback;
  } catch {
    return fallback;
  }
}

export function loadFlag(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

export function save(key: string, value: string | boolean) {
  try {
    localStorage.setItem(key, typeof value === "boolean" ? (value ? "1" : "0") : value);
  } catch {}
}

// Describes where a link in the preview goes, for the hover tooltip.

export type LinkInfo =
  | { kind: "web"; scheme: string; host: string; rest: string; idn: boolean; mismatch: string | null }
  | { kind: "mail"; address: string }
  | { kind: "anchor"; target: string }
  | { kind: "attachment"; id: string }
  | { kind: "unsupported"; href: string };

const DOMAIN_LIKE = /^(https?:\/\/)?([a-z0-9-]+\.)+[a-z]{2,}(:\d+)?([/?#]\S*)?$/i;

function bareHost(host: string): string {
  return host.toLowerCase().replace(/^www\./, "");
}

/**
 * If the visible link text looks like a URL/domain but points somewhere
 * else (a classic phishing trick), returns the domain the text claims.
 */
function claimedHost(text: string, actualHost: string): string | null {
  const t = text.trim();
  if (!DOMAIN_LIKE.test(t)) return null;
  try {
    const claimed = new URL(/^https?:\/\//i.test(t) ? t : `https://${t}`).hostname;
    return bareHost(claimed) === bareHost(actualHost) ? null : claimed;
  } catch {
    return null;
  }
}

export function describeLink(href: string, text: string): LinkInfo {
  const att = /^attachment:([0-9a-f-]{36})$/.exec(href);
  if (att) return { kind: "attachment", id: att[1] };
  if (href.startsWith("#")) {
    return { kind: "anchor", target: decodeURIComponent(href.slice(1)) };
  }
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return { kind: "unsupported", href };
  }
  if (url.protocol === "mailto:") {
    return { kind: "mail", address: decodeURIComponent(url.pathname) };
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    return { kind: "unsupported", href };
  }
  // URL() returns the punycode form (xn--...) for internationalized
  // domains, which exposes look-alike characters instead of hiding them.
  return {
    kind: "web",
    scheme: url.protocol.slice(0, -1),
    host: url.host,
    rest: url.pathname === "/" && !url.search && !url.hash ? "" : url.pathname + url.search + url.hash,
    idn: url.hostname.split(".").some((label) => label.startsWith("xn--")),
    mismatch: claimedHost(text, url.hostname),
  };
}

/** Same rule the click handler uses: only these leave the app. */
export function isOpenable(info: LinkInfo): boolean {
  return info.kind === "web" || info.kind === "mail";
}

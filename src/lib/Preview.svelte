<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { findAnchor, render } from "./markdown";
  import { renderMermaid } from "./mermaid";
  import { describeLink, isOpenable, type LinkInfo } from "./links";
  import { api, formatSize, type AttachmentMeta } from "./api";

  let {
    source,
    onscrollline,
    onrendered,
    onattachment,
  }: {
    source: string;
    /** Fired on scroll with the 0-based (fractional) source line at the top. */
    onscrollline?: (line: number) => void;
    /** Fired after new content is laid out (e.g. to re-sync scrolling). */
    onrendered?: () => void;
    /** An attachment link was clicked. */
    onattachment?: (id: string) => void;
  } = $props();
  let html = $derived(render(source));

  // ---------------------------------------------------- scroll sync support

  let scroller = $state<HTMLDivElement>();
  let article = $state<HTMLElement>();
  type Anchor = { line: number; top: number };
  let anchors: Anchor[] | null = null; // cached; null = needs recompute

  /** Block elements tagged with their source line, with their offsets. */
  function getAnchors(): Anchor[] {
    if (anchors) return anchors;
    if (!scroller || !article) return [];
    const base = scroller.getBoundingClientRect().top - scroller.scrollTop;
    const out: Anchor[] = [{ line: 0, top: 0 }];
    for (const el of article.querySelectorAll<HTMLElement>("[data-line]")) {
      const line = Number(el.dataset.line);
      const top = el.getBoundingClientRect().top - base;
      const last = out[out.length - 1];
      if (line > last.line && top >= last.top) out.push({ line, top });
    }
    out.push({ line: source.split("\n").length, top: scroller.scrollHeight });
    return (anchors = out);
  }

  function interpolate(list: Anchor[], value: number, from: "line" | "top"): number {
    const to = from === "line" ? "top" : "line";
    let i = 0;
    while (i < list.length - 2 && list[i + 1][from] <= value) i++;
    const a = list[i];
    const b = list[i + 1] ?? a;
    const span = b[from] - a[from];
    const t = span > 0 ? Math.min(Math.max((value - a[from]) / span, 0), 1) : 0;
    return a[to] + t * (b[to] - a[to]);
  }

  export function topLine(): number {
    return scroller ? interpolate(getAnchors(), scroller.scrollTop, "top") : 0;
  }

  export function scrollToLine(line: number) {
    if (scroller) scroller.scrollTop = interpolate(getAnchors(), line, "line");
  }

  // Layout changes (new content, images loading, resizing) invalidate anchors.
  $effect(() => {
    if (!article) return;
    const ro = new ResizeObserver(() => (anchors = null));
    ro.observe(article);
    return () => ro.disconnect();
  });
  $effect(() => {
    void html; // re-run after each render
    anchors = null;
    requestAnimationFrame(() => {
      onrendered?.();
      if (article) renderMermaid(article).catch(console.error);
    });
  });

  type Hover = {
    info: LinkInfo;
    title: string;
    x: number;
    y: number;
    above: boolean;
    attachment?: AttachmentMeta | null; // null = missing
  };
  let hover = $state<Hover | null>(null);
  let tip = $state<HTMLDivElement>();

  function linkFrom(target: EventTarget | null): HTMLAnchorElement | null {
    return target instanceof Element ? target.closest("a[href]") : null;
  }

  function show(a: HTMLAnchorElement) {
    const r = a.getBoundingClientRect();
    // Prefer below the link; flip above if it would run off the bottom.
    const above = r.bottom + 90 > window.innerHeight;
    hover = {
      info: describeLink(a.getAttribute("href") ?? "", a.textContent ?? ""),
      title: a.dataset.title ?? "",
      x: r.left,
      y: above ? r.top - 6 : r.bottom + 6,
      above,
    };
    const info = hover.info;
    if (info.kind === "attachment") {
      api
        .attachmentMeta(info.id)
        .then((m) => hover?.info === info && (hover.attachment = m))
        .catch(() => hover?.info === info && (hover.attachment = null));
    }
  }

  // Keep the tooltip inside the window horizontally once its width is known.
  $effect(() => {
    if (!hover || !tip) return;
    const overflow = tip.getBoundingClientRect().right - (window.innerWidth - 8);
    if (overflow > 0) tip.style.left = `${Math.max(8, hover.x - overflow)}px`;
  });

  function onover(e: MouseEvent | FocusEvent) {
    const a = linkFrom(e.target);
    if (a) show(a);
  }
  function onout(e: MouseEvent | FocusEvent) {
    const a = linkFrom(e.target);
    if (a && !a.contains(e.relatedTarget as Node | null)) hover = null;
  }

  // Scroll doesn't bubble and the scrolling element is our parent pane,
  // so listen in the capture phase for any scroll.
  $effect(() => {
    const hide = () => ((hover = null), (menu = null));
    document.addEventListener("scroll", hide, true);
    return () => document.removeEventListener("scroll", hide, true);
  });

  // Never let the webview navigate away; open external links in the browser.
  // ------------------------------------------------------ right-click menu
  // Replaces the webview's own menu on images and links (its "open in new
  // window" would navigate the app away).

  type Menu = { x: number; y: number; items: { label: string; run: () => void }[] };
  let menu = $state<Menu | null>(null);
  let menuEl = $state<HTMLDivElement>();

  function oncontextmenu(e: MouseEvent) {
    const target = e.target instanceof Element ? e.target : null;
    const img = target?.closest<HTMLElement>("img[data-attachment]");
    const link = linkFrom(target);
    if (!img && !link) return;
    e.preventDefault();
    hover = null;
    const items: Menu["items"] = [];
    const imageId = img?.dataset.attachment;
    if (imageId) items.push({ label: "Save image as…", run: () => onattachment?.(imageId) });
    if (link) {
      const href = link.getAttribute("href") ?? "";
      const info = describeLink(href, "");
      if (info.kind === "attachment") {
        if (info.id !== imageId) items.push({ label: "Save attachment as…", run: () => onattachment?.(info.id) });
      } else if (isOpenable(info)) {
        items.push({ label: "Open in browser", run: () => openUrl(href).catch(console.error) });
        items.push({
          label: info.kind === "mail" ? "Copy email address" : "Copy link address",
          run: () => navigator.clipboard.writeText(info.kind === "mail" ? info.address : href).catch(console.error),
        });
      }
    }
    menu = items.length ? { x: e.clientX, y: e.clientY, items } : null;
  }

  // Keep the menu inside the window once its size is known.
  $effect(() => {
    if (!menu || !menuEl) return;
    const r = menuEl.getBoundingClientRect();
    if (r.right > window.innerWidth - 8) menuEl.style.left = `${window.innerWidth - r.width - 8}px`;
    if (r.bottom > window.innerHeight - 8) menuEl.style.top = `${window.innerHeight - r.height - 8}px`;
  });

  function closeMenu(e?: Event) {
    if (menu && !(e?.target instanceof Node && menuEl?.contains(e.target))) menu = null;
  }

  function onclick(e: MouseEvent) {
    const a = linkFrom(e.target);
    if (!a) return;
    e.preventDefault();
    hover = null;
    const href = a.getAttribute("href") ?? "";
    const info = describeLink(href, a.textContent ?? "");
    if (isOpenable(info)) {
      openUrl(href).catch(console.error);
    } else if (info.kind === "attachment") {
      onattachment?.(info.id);
    } else if (info.kind === "anchor") {
      if (article) findAnchor(article, info.target)?.scrollIntoView();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions, a11y_mouse_events_have_key_events -->
<div class="scroller" bind:this={scroller} onscroll={() => onscrollline?.(topLine())}>
<article
  class="markdown"
  bind:this={article}
  {onclick}
  {oncontextmenu}
  onmouseover={onover}
  onmouseout={onout}
  onfocusin={onover}
  onfocusout={onout}
>
  {@html html}
</article>
</div>

<svelte:window
  onblur={() => ((hover = null), (menu = null))}
  onmousedown={closeMenu}
  onkeydown={(e) => e.key === "Escape" && (menu = null)}
/>

{#if menu}
  {@const m = menu}
  <div class="ctx-menu" role="menu" bind:this={menuEl} style:left="{m.x}px" style:top="{m.y}px">
    {#each m.items as item (item.label)}
      <button
        role="menuitem"
        onclick={() => {
          menu = null;
          item.run();
        }}>{item.label}</button
      >
    {/each}
  </div>
{/if}

{#if hover}
  {@const i = hover.info}
  <div
    class="link-tip"
    class:above={hover.above}
    class:warn={i.kind === "web" && i.mismatch}
    style:left="{hover.x}px"
    style:top="{hover.y}px"
    bind:this={tip}
    role="tooltip"
  >
    {#if hover.title}<div class="title">{hover.title}</div>{/if}
    {#if i.kind === "web"}
      <div class="url">
        <span class="dim">{i.scheme}://</span><strong>{i.host}</strong><span class="dim">{i.rest}</span>
      </div>
      {#if i.scheme === "http"}<div class="note">Not encrypted (http)</div>{/if}
      {#if i.idn}<div class="note">Internationalized domain, check for look-alike characters</div>{/if}
      {#if i.mismatch}
        <div class="alert">⚠ Text says <strong>{i.mismatch}</strong>, but the link goes to <strong>{i.host}</strong></div>
      {/if}
      <div class="hint">Click to open in your browser</div>
    {:else if i.kind === "mail"}
      <div class="url"><span class="dim">Email </span><strong>{i.address}</strong></div>
      <div class="hint">Click to open in your mail app</div>
    {:else if i.kind === "attachment"}
      {#if hover.attachment}
        <div class="url"><span class="dim">📎 </span><strong>{hover.attachment.name}</strong></div>
        <div class="note">{hover.attachment.mime} · {formatSize(hover.attachment.size)}</div>
        <div class="hint">Click to save a decrypted copy…</div>
      {:else if hover.attachment === null}
        <div class="alert">Attachment is missing from the vault</div>
      {:else}
        <div class="url dim">📎 Attachment…</div>
      {/if}
    {:else if i.kind === "anchor"}
      <div class="url"><span class="dim">Jump to </span><strong>#{i.target}</strong></div>
    {:else}
      <div class="url dim">{i.href}</div>
      <div class="note">This link type can't be opened</div>
    {/if}
  </div>
{/if}

<style>
  .ctx-menu {
    position: fixed;
    z-index: 60;
    min-width: 11rem;
    padding: 0.3rem;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.3);
  }
  .ctx-menu button {
    display: block;
    width: 100%;
    border: none;
    background: none;
    text-align: left;
    padding: 0.4rem 0.6rem;
    font-size: 0.88rem;
    border-radius: 5px;
  }
  .ctx-menu button:hover {
    background: var(--hover);
  }
  .scroller {
    height: 100%;
    overflow: auto;
  }
  .link-tip {
    position: fixed;
    z-index: 50;
    max-width: min(36rem, calc(100vw - 16px));
    padding: 0.45rem 0.65rem;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.25);
    font-size: 0.82rem;
    line-height: 1.4;
    pointer-events: none;
  }
  .link-tip.above {
    transform: translateY(-100%);
  }
  .link-tip.warn {
    border-color: var(--danger);
  }
  .url {
    font-family: var(--mono);
    word-break: break-all;
  }
  .title {
    font-weight: 600;
    margin-bottom: 0.2rem;
  }
  .dim {
    color: var(--muted);
  }
  .note,
  .hint {
    color: var(--muted);
    font-size: 0.75rem;
    margin-top: 0.2rem;
  }
  .alert {
    color: var(--danger);
    margin-top: 0.3rem;
  }
</style>

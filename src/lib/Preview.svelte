<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { render } from "./markdown";
  import { describeLink, isOpenable, type LinkInfo } from "./links";

  let { source }: { source: string } = $props();
  let html = $derived(render(source));

  type Hover = { info: LinkInfo; title: string; x: number; y: number; above: boolean };
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
    const hide = () => (hover = null);
    document.addEventListener("scroll", hide, true);
    return () => document.removeEventListener("scroll", hide, true);
  });

  // Never let the webview navigate away; open external links in the browser.
  function onclick(e: MouseEvent) {
    const a = linkFrom(e.target);
    if (!a) return;
    e.preventDefault();
    hover = null;
    const href = a.getAttribute("href") ?? "";
    const info = describeLink(href, a.textContent ?? "");
    if (isOpenable(info)) {
      openUrl(href).catch(console.error);
    } else if (info.kind === "anchor") {
      document.getElementById(info.target)?.scrollIntoView();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions, a11y_mouse_events_have_key_events -->
<article
  class="markdown"
  {onclick}
  onmouseover={onover}
  onmouseout={onout}
  onfocusin={onover}
  onfocusout={onout}
>
  {@html html}
</article>

<svelte:window onblur={() => (hover = null)} />

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
    {:else if i.kind === "anchor"}
      <div class="url"><span class="dim">Jump to </span><strong>#{i.target}</strong></div>
    {:else}
      <div class="url dim">{i.href}</div>
      <div class="note">This link type can't be opened</div>
    {/if}
  </div>
{/if}

<style>
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

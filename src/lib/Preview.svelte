<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { render } from "./markdown";

  let { source }: { source: string } = $props();
  let html = $derived(render(source));

  // Never let the webview navigate away; open external links in the browser.
  function onclick(e: MouseEvent) {
    const a = (e.target as Element).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (/^(https?:|mailto:)/i.test(href)) {
      openUrl(href).catch(console.error);
    } else if (href.startsWith("#")) {
      document.getElementById(decodeURIComponent(href.slice(1)))?.scrollIntoView();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<article class="markdown" {onclick}>{@html html}</article>

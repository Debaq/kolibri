<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import MailApp from "$lib/MailApp.svelte";
  import type { Service } from "$lib/mail";

  let services = $state<Service[]>([]);
  let activeId = $state<string | null>(null);

  let active = $derived(
    activeId ? services.find((s) => s.id === activeId) ?? null : null,
  );

  let unlistens: UnlistenFn[] = [];

  async function reload() {
    try {
      services = await invoke<Service[]>("list_services");
      activeId = await invoke<string | null>("get_active_service");
    } catch (e) {
      console.error("[content] reload error", e);
    }
  }

  onMount(async () => {
    await reload();
    unlistens.push(
      await listen<string | null>("kolibri:active_changed", (e) => {
        activeId = e.payload ?? null;
      }),
      await listen("kolibri:services_changed", () => {
        reload();
      }),
    );
  });

  onDestroy(() => {
    for (const u of unlistens) u();
  });
</script>

{#if active && (active.engine === "imap" || active.engine === "graph")}
  {#if active.engine === "imap" && !active.imap?.authorized}
    <div class="empty">
      <div class="icon">⚠</div>
      <h3>{active.name}</h3>
      <p>Servicio sin autorizar. Corré <code>imap_oauth_authorize</code> desde devtools.</p>
    </div>
  {:else if active.engine === "graph" && !active.graph?.authorized}
    <div class="empty">
      <div class="icon">⚠</div>
      <h3>{active.name}</h3>
      <p>Servicio Graph sin autorizar (step 5).</p>
    </div>
  {:else}
    <MailApp serviceId={active.id} />
  {/if}
{:else}
  <div class="empty">
    <img src="/logo.webp" alt="Kolibri" />
  </div>
{/if}

<style>
  :global(html), :global(body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: #1a1a1a;
    color: #e0e0e0;
  }
  :global(#svelte) {
    width: 100%;
    height: 100%;
  }

  .empty {
    width: 100%;
    height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    background: #1a1a1a;
    color: #aaa;
    text-align: center;
    padding: 24px;
    box-sizing: border-box;
  }
  .empty img {
    max-width: min(40vw, 320px);
    max-height: min(60vh, 320px);
    opacity: 0.85;
  }
  .empty .icon {
    font-size: 64px;
    margin-bottom: 16px;
    color: #d97706;
  }
  .empty h3 { margin: 8px 0; color: #e0e0e0; }
  .empty p { color: #888; max-width: 480px; }
  .empty code {
    background: #2a2a2a;
    padding: 2px 6px;
    border-radius: 3px;
    font-family: monospace;
  }
</style>

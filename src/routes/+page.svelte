<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import AddServiceModal from "$lib/AddServiceModal.svelte";

  type Service = {
    id: string;
    name: string;
    url: string;
    icon: string | null;
    color: string | null;
  };

  let services = $state<Service[]>([]);
  let activeId = $state<string | null>(null);
  let showAdd = $state(false);
  let unlisteners: UnlistenFn[] = [];

  async function refresh() {
    services = await invoke<Service[]>("list_services");
    activeId = await invoke<string | null>("get_active_service");
  }

  async function onPick(svc: { name: string; url: string; icon: string | null; color?: string }) {
    showAdd = false;
    const created = await invoke<Service>("add_service", {
      name: svc.name,
      url: svc.url,
      icon: svc.icon,
      color: svc.color ?? null,
    });
    await refresh();
    await invoke("switch_service", { id: created.id });
    activeId = created.id;
  }

  async function selectService(id: string) {
    await invoke("switch_service", { id });
    activeId = id;
  }

  async function showHome() {
    await invoke("switch_service", { id: null });
    activeId = null;
  }

  async function removeService(id: string, e: Event) {
    e.stopPropagation();
    if (!confirm("¿Eliminar servicio?")) return;
    await invoke("remove_service", { id });
    await refresh();
  }

  function initialOf(s: Service): string {
    if (s.icon && s.icon.length > 0) return s.icon.charAt(0);
    return s.name.charAt(0).toUpperCase();
  }

  const minimize = () => invoke("window_minimize");
  const toggleMax = () => invoke("window_toggle_maximize");
  const closeWin = () => invoke("window_close");

  function dragOn(e: MouseEvent) {
    if (e.button !== 0) return;
    const t = e.target as HTMLElement;
    if (t.closest("button, .tab-close, [data-no-drag]")) return;
    if (e.detail === 2) {
      toggleMax();
      return;
    }
    getCurrentWindow().startDragging().catch(console.error);
  }

  onMount(async () => {
    await refresh();
    unlisteners.push(await listen("kolibri:services_changed", refresh));
    unlisteners.push(await listen("kolibri:active_changed", refresh));
  });

  onDestroy(() => unlisteners.forEach((u) => u()));
</script>

<div class="bar" data-tauri-drag-region onmousedown={dragOn}>
  <button class="add" onclick={() => (showAdd = true)} title="Agregar servicio">＋</button>

  <div class="tabs" data-tauri-drag-region>
    {#each services as s (s.id)}
      <button
        class="tab"
        class:active={s.id === activeId}
        onclick={() => selectService(s.id)}
        title={s.name}
      >
        <span class="tab-icon" style:background={s.color ?? "#444"}>{initialOf(s)}</span>
        <span class="tab-name">{s.name}</span>
        <span
          class="tab-close"
          role="button"
          tabindex="0"
          onclick={(e) => removeService(s.id, e)}
          onkeydown={(e) => e.key === "Enter" && removeService(s.id, e)}
          aria-label="Eliminar"
        >×</span>
      </button>
    {/each}
  </div>

  <div class="spacer" data-tauri-drag-region></div>

  <div class="controls">
    <button class="ctrl" onclick={showHome} title="Inicio">⌂</button>
    <button class="ctrl" onclick={() => alert("Config (próximamente)")} title="Configuración">⚙</button>
    <button class="ctrl" onclick={minimize} title="Minimizar">─</button>
    <button class="ctrl" onclick={toggleMax} title="Maximizar">▢</button>
    <button class="ctrl close" onclick={closeWin} title="Cerrar">×</button>
  </div>
</div>

<main class="welcome">
  {#if services.length === 0}
    <h1>Bienvenido a Kolibri</h1>
    <p>Agrega tu primer servicio para empezar</p>
    <button class="cta" onclick={() => (showAdd = true)}>+ Agregar servicio</button>
  {:else if !activeId}
    <h1>Kolibri</h1>
    <p>Selecciona un servicio en la barra de arriba</p>
  {/if}
</main>

{#if showAdd}
  <AddServiceModal onclose={() => (showAdd = false)} onpick={onPick} />
{/if}

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    height: 100%;
    overflow: hidden;
    font-family: Inter, system-ui, sans-serif;
    background: #1a1a1a;
    color: #eee;
    user-select: none;
  }

  .bar {
    display: flex;
    align-items: center;
    height: 44px;
    background: #222;
    border-bottom: 1px solid #2c2c2c;
    padding: 0 6px;
    gap: 4px;
    box-sizing: border-box;
  }
  .bar * { box-sizing: border-box; }

  .add {
    background: #2c2c2c;
    border: 1px solid #3a3a3a;
    color: #ccc;
    width: 34px;
    height: 34px;
    border-radius: 8px;
    font-size: 18px;
    cursor: pointer;
    flex-shrink: 0;
    line-height: 1;
  }
  .add:hover { background: #383838; color: #fff; }

  .tabs {
    display: flex;
    gap: 2px;
    overflow-x: auto;
    flex-shrink: 1;
    min-width: 0;
    height: 100%;
    align-items: center;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 8px 0 4px;
    background: #2a2a2a;
    border: 1px solid transparent;
    border-radius: 8px;
    color: #ccc;
    cursor: pointer;
    font-size: 12px;
    flex-shrink: 0;
    max-width: 180px;
  }
  .tab:hover { background: #333; }
  .tab.active {
    background: #383838;
    border-color: #4a4a4a;
    color: #fff;
  }
  .tab-icon {
    width: 26px;
    height: 26px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: white;
    font-weight: 700;
    font-size: 12px;
    flex-shrink: 0;
  }
  .tab-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab-close {
    color: #777;
    font-size: 14px;
    line-height: 1;
    padding: 2px 4px;
    border-radius: 4px;
    cursor: pointer;
  }
  .tab-close:hover { background: #4a2222; color: #f88; }

  .spacer { flex: 1; min-width: 12px; height: 100%; }

  .controls { display: flex; height: 100%; align-items: center; flex-shrink: 0; }
  .ctrl {
    width: 38px;
    height: 100%;
    background: transparent;
    border: none;
    color: #aaa;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
  }
  .ctrl:hover { background: #333; color: #fff; }
  .ctrl.close:hover { background: #c00; color: #fff; }

  .welcome {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    height: calc(100vh - 44px);
    color: #aaa;
    text-align: center;
  }
  .welcome h1 { margin: 0; font-weight: 500; color: #ddd; }
  .welcome p { margin: 0; }
  .cta {
    margin-top: 8px;
    padding: 12px 22px;
    background: #4a8;
    border: none;
    border-radius: 8px;
    color: white;
    font-weight: 600;
    cursor: pointer;
    font-size: 14px;
  }
  .cta:hover { background: #5b9; }
</style>

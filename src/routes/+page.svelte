<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { CATALOG, type ServiceTemplate } from "$lib/catalog";

  type Service = {
    id: string;
    name: string;
    url: string;
    icon: string | null;
    color: string | null;
  };

  type Mode = "tabs" | "picker" | "custom" | "remove";

  let services = $state<Service[]>([]);
  let activeId = $state<string | null>(null);
  let mode = $state<Mode>("tabs");
  let customName = $state("");
  let customUrl = $state("");
  let pendingRemoveId = $state<string | null>(null);
  let unlisteners: UnlistenFn[] = [];

  async function refresh() {
    services = await invoke<Service[]>("list_services");
    activeId = await invoke<string | null>("get_active_service");
  }

  async function addFromTemplate(t: ServiceTemplate) {
    const created = await invoke<Service>("add_service", {
      name: t.name,
      url: t.url,
      icon: t.initial,
      color: t.color,
    });
    mode = "tabs";
    await refresh();
    await invoke("switch_service", { id: created.id });
    activeId = created.id;
  }

  async function addCustom(e: Event) {
    e.preventDefault();
    if (!customName.trim() || !customUrl.trim()) return;
    let url = customUrl.trim();
    if (!/^https?:\/\//i.test(url)) url = "https://" + url;
    const created = await invoke<Service>("add_service", {
      name: customName.trim(),
      url,
      icon: null,
      color: null,
    });
    customName = "";
    customUrl = "";
    mode = "tabs";
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

  function askRemove(id: string) {
    pendingRemoveId = id;
  }

  async function confirmRemove() {
    if (!pendingRemoveId) return;
    await invoke("remove_service", { id: pendingRemoveId });
    pendingRemoveId = null;
    mode = "tabs";
    await refresh();
  }

  function startRemove() {
    pendingRemoveId = null;
    mode = "remove";
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
    if (t.closest("button, input, .tab-close, [data-no-drag]")) return;
    if (e.detail === 2) {
      toggleMax();
      return;
    }
    getCurrentWindow().startDragging().catch(console.error);
  }

  function startPicker() {
    mode = "picker";
  }
  function cancelPicker() {
    mode = "tabs";
    customName = "";
    customUrl = "";
  }

  function cancelRemove() {
    pendingRemoveId = null;
    mode = "tabs";
  }

  function cancelConfirm() {
    pendingRemoveId = null;
  }

  onMount(async () => {
    await refresh();
    unlisteners.push(await listen("kolibri:services_changed", refresh));
    unlisteners.push(await listen("kolibri:active_changed", refresh));
  });

  onDestroy(() => unlisteners.forEach((u) => u()));
</script>

<div class="bar" data-tauri-drag-region onmousedown={dragOn}>
  {#if mode === "tabs"}
    <button class="add" onclick={startPicker} title="Agregar servicio">＋</button>

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
        </button>
      {/each}
    </div>
  {:else if mode === "remove"}
    <button class="add cancel" onclick={cancelRemove} title="Cancelar">×</button>
    {#if pendingRemoveId}
      {@const target = services.find((s) => s.id === pendingRemoveId)}
      {#if target}
        <div class="picker confirm-row">
          <div class="pick remove-pick confirm-target" aria-disabled="true">
            <span class="pick-icon" style:background={target.color ?? "#444"}>{initialOf(target)}</span>
            <span class="pick-name">{target.name}</span>
          </div>
          <span class="confirm-text">¿Realmente quiere eliminar?</span>
          <button class="confirm-yes" onclick={confirmRemove}>Aceptar</button>
          <button class="confirm-no" onclick={cancelRemove}>Cancelar</button>
        </div>
      {/if}
    {:else}
      <div class="picker">
        {#each services as s (s.id)}
          <button
            class="pick remove-pick"
            onclick={() => askRemove(s.id)}
            title="Eliminar {s.name}"
          >
            <span class="pick-icon" style:background={s.color ?? "#444"}>{initialOf(s)}</span>
            <span class="pick-name">{s.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {:else if mode === "picker"}
    <button class="add cancel" onclick={cancelPicker} title="Cancelar">×</button>
    <div class="picker">
      {#each CATALOG as t (t.key)}
        <button
          class="pick"
          onclick={() => addFromTemplate(t)}
          title="{t.name} — {t.description}"
        >
          <span class="pick-icon" style:background={t.color}>{t.initial}</span>
          <span class="pick-name">{t.name}</span>
        </button>
      {/each}
      <button class="pick custom-btn" onclick={() => (mode = "custom")} title="URL personalizada">
        <span class="pick-icon" style:background="#555">…</span>
        <span class="pick-name">Otra</span>
      </button>
    </div>
  {:else if mode === "custom"}
    <button class="add cancel" onclick={cancelPicker} title="Cancelar">×</button>
    <form class="custom-form" onsubmit={addCustom}>
      <input
        type="text"
        placeholder="Nombre"
        bind:value={customName}
        data-no-drag
        required
      />
      <input
        type="text"
        placeholder="https://ejemplo.com"
        bind:value={customUrl}
        data-no-drag
        required
      />
      <button type="submit" class="custom-go">Agregar</button>
    </form>
  {/if}

  <div class="spacer" data-tauri-drag-region></div>

  <div class="controls">
    <button class="ctrl" onclick={showHome} title="Inicio">⌂</button>
    <button class="ctrl" onclick={() => alert("Config (próximamente)")} title="Configuración">⚙</button>
    {#if services.length > 0}
      <button class="ctrl ctrl-remove" onclick={startRemove} title="Eliminar servicio">🗑</button>
    {/if}
    <button class="ctrl" onclick={minimize} title="Minimizar">─</button>
    <button class="ctrl" onclick={toggleMax} title="Maximizar">▢</button>
    <button class="ctrl close" onclick={closeWin} title="Cerrar">×</button>
  </div>
</div>

<main class="welcome">
  {#if services.length === 0}
    <h1>Bienvenido a Kolibri</h1>
    <p>Click ＋ arriba para agregar tu primer servicio</p>
  {:else if !activeId}
    <h1>Kolibri</h1>
    <p>Selecciona un servicio en la barra de arriba</p>
  {/if}
</main>

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
    height: 56px;
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
  .add.cancel { background: #4a2222; color: #f88; border-color: #6a3a3a; }
  .add.cancel:hover { background: #6a2828; }

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
  .tab-icon, .pick-icon {
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
  .tab-name, .pick-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
  }
  .ctrl-remove:hover { background: #4a2222; color: #f88; }

  .remove-pick { border-color: #5a2a2a; }
  .remove-pick:hover { background: #4a2222; border-color: #8a3a3a; color: #fdd; }

  .confirm-row { gap: 10px; }
  .confirm-target { cursor: default; pointer-events: none; opacity: 0.85; }
  .confirm-text { color: #f88; font-size: 12px; white-space: nowrap; }
  .confirm-yes, .confirm-no {
    height: 30px;
    padding: 0 14px;
    border-radius: 6px;
    border: none;
    font-weight: 600;
    cursor: pointer;
    font-size: 12px;
  }
  .confirm-yes { background: #c33; color: #fff; }
  .confirm-yes:hover { background: #d44; }
  .confirm-no { background: #2c2c2c; color: #ccc; border: 1px solid #3a3a3a; }
  .confirm-no:hover { background: #383838; color: #fff; }

  .picker {
    display: flex;
    gap: 4px;
    overflow-x: auto;
    flex: 1;
    min-width: 0;
    height: 100%;
    align-items: center;
    padding: 0 4px;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 8px 0 4px;
    background: #2a2a2a;
    border: 1px solid #3a3a3a;
    border-radius: 8px;
    color: #ddd;
    cursor: pointer;
    flex-shrink: 0;
  }
  .pick:hover { background: #383838; border-color: #555; transform: translateY(-1px); }
  .pick:active { transform: translateY(0); }

  .custom-form {
    display: flex;
    gap: 6px;
    flex: 1;
    align-items: center;
    height: 100%;
    padding: 0 4px;
  }
  .custom-form input {
    background: #1d1d1d;
    border: 1px solid #3a3a3a;
    border-radius: 6px;
    padding: 0 10px;
    height: 30px;
    color: #eee;
    font-size: 12px;
    outline: none;
  }
  .custom-form input:focus { border-color: #6a8; }
  .custom-form input:first-of-type { width: 140px; }
  .custom-form input:nth-of-type(2) { flex: 1; min-width: 200px; }
  .custom-go {
    height: 30px;
    padding: 0 14px;
    background: #4a8;
    color: white;
    border: none;
    border-radius: 6px;
    font-weight: 600;
    cursor: pointer;
    font-size: 12px;
  }
  .custom-go:hover { background: #5b9; }

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
    height: calc(100vh - 56px);
    color: #aaa;
    text-align: center;
  }
  .welcome h1 { margin: 0; font-weight: 500; color: #ddd; }
  .welcome p { margin: 0; }
</style>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { CATALOG, type ServiceTemplate } from "$lib/catalog";
  import SettingsPanel from "$lib/SettingsPanel.svelte";

  type Theme = "dark" | "light";

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
  let dragId = $state<string | null>(null);
  let dragOverId = $state<string | null>(null);
  let showSettings = $state(false);
  let loadingIds = $state<Set<string>>(new Set());

  function setLoading(id: string, loading: boolean) {
    const next = new Set(loadingIds);
    if (loading) next.add(id);
    else next.delete(id);
    loadingIds = next;
  }

  async function reloadActive() {
    if (!activeId) return;
    setLoading(activeId, true);
    await invoke("reload_service", { id: activeId });
  }
  let theme = $state<Theme>(((typeof localStorage !== "undefined" && (localStorage.getItem("kolibri:theme") as Theme)) || "dark"));
  let faviconFailed = $state<Set<string>>(new Set());

  function applyTheme(t: Theme) {
    theme = t;
    if (typeof document !== "undefined") {
      document.documentElement.dataset.theme = t;
    }
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("kolibri:theme", t);
    }
  }
  let unlisteners: UnlistenFn[] = [];

  function faviconUrl(url: string): string | null {
    try {
      const host = new URL(url).hostname;
      return `https://www.google.com/s2/favicons?domain=${host}&sz=64`;
    } catch {
      return null;
    }
  }

  function markFaviconFailed(key: string) {
    if (faviconFailed.has(key)) return;
    const next = new Set(faviconFailed);
    next.add(key);
    faviconFailed = next;
  }

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

  function onTabDragStart(e: DragEvent, id: string) {
    dragId = id;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", id);
    }
  }
  function onTabDragOver(e: DragEvent, id: string) {
    if (!dragId || dragId === id) return;
    e.preventDefault();
    dragOverId = id;
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }
  function onTabDragLeave(id: string) {
    if (dragOverId === id) dragOverId = null;
  }
  async function onTabDrop(e: DragEvent, targetId: string) {
    e.preventDefault();
    const src = dragId;
    dragId = null;
    dragOverId = null;
    if (!src || src === targetId) return;
    const ids = services.map((s) => s.id);
    const from = ids.indexOf(src);
    const to = ids.indexOf(targetId);
    if (from < 0 || to < 0) return;
    ids.splice(to, 0, ...ids.splice(from, 1));
    services = ids.map((id) => services.find((s) => s.id === id)!).filter(Boolean);
    await invoke("reorder_services", { ids });
  }
  function onTabDragEnd() {
    dragId = null;
    dragOverId = null;
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

  function onKey(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
    const k = e.key.toLowerCase();
    if (k >= "1" && k <= "9") {
      const idx = parseInt(k, 10) - 1;
      if (idx < services.length) {
        e.preventDefault();
        selectService(services[idx].id);
      }
    } else if (k === "t") {
      e.preventDefault();
      startPicker();
    } else if (k === "w") {
      if (activeId) {
        e.preventDefault();
        askRemove(activeId);
        mode = "remove";
      }
    } else if (k === "h" || k === "0") {
      e.preventDefault();
      showHome();
    }
  }

  onMount(async () => {
    applyTheme(theme);
    await refresh();
    unlisteners.push(await listen("kolibri:services_changed", refresh));
    unlisteners.push(await listen("kolibri:active_changed", refresh));
    unlisteners.push(
      await listen<[string, string]>("kolibri:page_load", (e) => {
        const [sid, phase] = e.payload;
        setLoading(sid, phase === "started");
      })
    );
    window.addEventListener("keydown", onKey);
  });

  onDestroy(() => {
    unlisteners.forEach((u) => u());
    window.removeEventListener("keydown", onKey);
  });
</script>

<div class="bar" data-tauri-drag-region onmousedown={dragOn}>
  {#if mode === "tabs"}
    <button class="add" onclick={startPicker} title="Agregar servicio">＋</button>

    <div class="tabs" data-tauri-drag-region>
      {#each services as s (s.id)}
        {@const fav = faviconUrl(s.url)}
        {@const useFav = fav && !faviconFailed.has("svc:" + s.id)}
        <button
          class="tab"
          class:active={s.id === activeId}
          class:drag-over={dragOverId === s.id}
          class:dragging={dragId === s.id}
          draggable="true"
          ondragstart={(e) => onTabDragStart(e, s.id)}
          ondragover={(e) => onTabDragOver(e, s.id)}
          ondragleave={() => onTabDragLeave(s.id)}
          ondrop={(e) => onTabDrop(e, s.id)}
          ondragend={onTabDragEnd}
          onclick={() => selectService(s.id)}
          title={s.name}
        >
          <span class="tab-icon" class:img={useFav} class:loading={loadingIds.has(s.id)} style:background={useFav ? "transparent" : (s.color ?? "#444")}>
            {#if loadingIds.has(s.id)}
              <span class="spin"></span>
            {:else if useFav}
              <img src={fav} alt="" onerror={() => markFaviconFailed("svc:" + s.id)} />
            {:else}
              {initialOf(s)}
            {/if}
          </span>
          <span class="tab-name">{s.name}</span>
        </button>
      {/each}
    </div>
  {:else if mode === "remove"}
    <button class="add cancel" onclick={cancelRemove} title="Cancelar">×</button>
    {#if pendingRemoveId}
      {@const target = services.find((s) => s.id === pendingRemoveId)}
      {#if target}
        {@const tFav = faviconUrl(target.url)}
        {@const tUse = tFav && !faviconFailed.has("svc:" + target.id)}
        <div class="picker confirm-row">
          <div class="pick remove-pick confirm-target" aria-disabled="true">
            <span class="pick-icon" class:img={tUse} style:background={tUse ? "transparent" : (target.color ?? "#444")}>
              {#if tUse}
                <img src={tFav} alt="" onerror={() => markFaviconFailed("svc:" + target.id)} />
              {:else}
                {initialOf(target)}
              {/if}
            </span>
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
          {@const fav = faviconUrl(s.url)}
          {@const useFav = fav && !faviconFailed.has("svc:" + s.id)}
          <button
            class="pick remove-pick"
            onclick={() => askRemove(s.id)}
            title="Eliminar {s.name}"
          >
            <span class="pick-icon" class:img={useFav} style:background={useFav ? "transparent" : (s.color ?? "#444")}>
              {#if useFav}
                <img src={fav} alt="" onerror={() => markFaviconFailed("svc:" + s.id)} />
              {:else}
                {initialOf(s)}
              {/if}
            </span>
            <span class="pick-name">{s.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {:else if mode === "picker"}
    <button class="add cancel" onclick={cancelPicker} title="Cancelar">×</button>
    <div class="picker">
      {#each CATALOG as t (t.key)}
        {@const fav = faviconUrl(t.url)}
        {@const useFav = fav && !faviconFailed.has("tpl:" + t.key)}
        <button
          class="pick"
          onclick={() => addFromTemplate(t)}
          title="{t.name} — {t.description}"
        >
          <span class="pick-icon" class:img={useFav} style:background={useFav ? "transparent" : t.color}>
            {#if useFav}
              <img src={fav} alt="" onerror={() => markFaviconFailed("tpl:" + t.key)} />
            {:else}
              {t.initial}
            {/if}
          </span>
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
    {#if activeId}
      <button class="ctrl" onclick={reloadActive} title="Recargar">↻</button>
    {/if}
    <button class="ctrl" onclick={() => (showSettings = true)} title="Configuración">⚙</button>
    {#if services.length > 0}
      <button class="ctrl ctrl-remove" onclick={startRemove} title="Eliminar servicio">🗑</button>
    {/if}
    <button class="ctrl" onclick={minimize} title="Minimizar">─</button>
    <button class="ctrl" onclick={toggleMax} title="Maximizar">▢</button>
    <button class="ctrl close" onclick={closeWin} title="Cerrar">×</button>
  </div>
</div>

{#if showSettings}
  <SettingsPanel
    {services}
    {theme}
    onClose={() => (showSettings = false)}
    onChanged={refresh}
    onTheme={applyTheme}
  />
{/if}

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
    background: var(--kolibri-bg, #1a1a1a);
    color: var(--kolibri-fg, #eee);
    user-select: none;
  }
  :global(html[data-theme="light"]) {
    --kolibri-bg: #f3f3f3;
    --kolibri-fg: #222;
    --kolibri-bar: #ffffff;
    --kolibri-bar-border: #d8d8d8;
  }
  :global(html[data-theme="dark"]) {
    --kolibri-bg: #1a1a1a;
    --kolibri-fg: #eee;
    --kolibri-bar: #222;
    --kolibri-bar-border: #2c2c2c;
  }

  .bar {
    display: flex;
    align-items: center;
    height: 56px;
    background: var(--kolibri-bar, #222);
    border-bottom: 1px solid var(--kolibri-bar-border, #2c2c2c);
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
  .tab.dragging { opacity: 0.4; }
  .tab.drag-over { border-color: #6a8; box-shadow: -2px 0 0 #6a8; }
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
    overflow: hidden;
  }
  .tab-icon.img, .pick-icon.img { padding: 3px; background: #fff !important; }
  .tab-icon.loading { background: #2c2c2c !important; }
  .spin {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid #555;
    border-top-color: #6a8;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .tab-icon img, .pick-icon img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
    -webkit-user-drag: none;
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

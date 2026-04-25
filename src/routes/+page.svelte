<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { CATALOG, type ServiceTemplate } from "$lib/catalog";
  import { isEnabled as autostartIsEnabled, enable as autostartEnable, disable as autostartDisable } from "@tauri-apps/plugin-autostart";

  type Theme = "dark" | "light";

  type Service = {
    id: string;
    name: string;
    url: string;
    icon: string | null;
    color: string | null;
  };

  type Mode = "tabs" | "picker" | "custom" | "remove" | "settings" | "edit" | "shortcuts";

  let services = $state<Service[]>([]);
  let activeId = $state<string | null>(null);
  let mode = $state<Mode>("tabs");
  let customName = $state("");
  let customUrl = $state("");
  let pendingRemoveId = $state<string | null>(null);
  let dragId = $state<string | null>(null);
  let dragOverId = $state<string | null>(null);
  let editingId = $state<string | null>(null);
  let editName = $state("");
  let editUrl = $state("");
  let editColor = $state("");
  let editIcon = $state("");
  let loadingIds = $state<Set<string>>(new Set());
  let unread = $state<Record<string, number>>({});
  let autostartOn = $state(false);
  let toggleShortcut = $state("Ctrl+Alt+K");
  let shortcutDraft = $state("");
  let shortcutMsg = $state("");

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

  function openSettings() {
    mode = "settings";
  }

  function startEdit(s: Service) {
    editingId = s.id;
    editName = s.name;
    editUrl = s.url;
    editColor = s.color ?? "";
    editIcon = s.icon ?? "";
    mode = "edit";
  }

  async function saveEdit() {
    if (!editingId) return;
    await invoke("update_service", {
      id: editingId,
      name: editName.trim(),
      url: editUrl.trim(),
      icon: editIcon,
      color: editColor,
    });
    editingId = null;
    mode = "settings";
    await refresh();
  }

  function cancelEdit() {
    editingId = null;
    mode = "settings";
  }

  async function moveService(id: string, delta: number) {
    const ids = services.map((s) => s.id);
    const i = ids.indexOf(id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    await invoke("reorder_services", { ids });
    await refresh();
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

  async function toggleAutostart() {
    if (autostartOn) {
      await autostartDisable();
      autostartOn = false;
    } else {
      await autostartEnable();
      autostartOn = true;
    }
  }

  async function loadShortcut() {
    toggleShortcut = await invoke<string>("get_toggle_shortcut");
    shortcutDraft = toggleShortcut;
  }

  async function saveShortcut() {
    const acc = shortcutDraft.trim();
    if (!acc) return;
    try {
      await invoke("set_toggle_shortcut", { accelerator: acc });
      toggleShortcut = acc;
      shortcutMsg = "OK";
    } catch (e) {
      shortcutMsg = "Error: " + e;
    }
    setTimeout(() => (shortcutMsg = ""), 2000);
  }

  function captureShortcut(e: KeyboardEvent) {
    if (e.key === "Tab" || e.key === "Escape") return;
    e.preventDefault();
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    const k = e.key;
    if (!["Control", "Alt", "Shift", "Meta"].includes(k)) {
      parts.push(k.length === 1 ? k.toUpperCase() : k);
    }
    if (parts.length > 0) shortcutDraft = parts.join("+");
  }

  function startResize(dir: string) {
    return (e: MouseEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      // @ts-ignore - tauri ResizeDirection enum
      getCurrentWindow().startResizeDragging(dir).catch(console.error);
    };
  }

  onMount(async () => {
    applyTheme(theme);
    await refresh();
    try {
      autostartOn = await autostartIsEnabled();
    } catch {}
    try {
      await loadShortcut();
    } catch {}
    unlisteners.push(await listen("kolibri:services_changed", refresh));
    unlisteners.push(await listen("kolibri:active_changed", refresh));
    unlisteners.push(
      await listen<[string, string]>("kolibri:page_load", (e) => {
        const [sid, phase] = e.payload;
        setLoading(sid, phase === "started");
      })
    );
    unlisteners.push(
      await listen<{ service_id: string; count: number; total: number }>(
        "kolibri:unread",
        (e) => {
          const { service_id, count } = e.payload;
          const next = { ...unread };
          if (count > 0) next[service_id] = count;
          else delete next[service_id];
          unread = next;
        }
      )
    );
    window.addEventListener("keydown", onKey);
  });

  onDestroy(() => {
    unlisteners.forEach((u) => u());
    window.removeEventListener("keydown", onKey);
  });
</script>

<div class="rz rz-s" onmousedown={startResize("South")}></div>
<div class="rz rz-e" onmousedown={startResize("East")}></div>
<div class="rz rz-w" onmousedown={startResize("West")}></div>
<div class="rz rz-se" onmousedown={startResize("SouthEast")}></div>
<div class="rz rz-sw" onmousedown={startResize("SouthWest")}></div>

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
          {#if unread[s.id]}
            <span class="badge">{unread[s.id] > 99 ? "99+" : unread[s.id]}</span>
          {/if}
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
  {:else if mode === "settings"}
    <button class="add cancel" onclick={() => (mode = "tabs")} title="Cerrar">×</button>
    <div class="picker">
      <span class="seg-label">Tema</span>
      <button class="seg" class:on={theme === "dark"} onclick={() => applyTheme("dark")}>Oscuro</button>
      <button class="seg" class:on={theme === "light"} onclick={() => applyTheme("light")}>Claro</button>
      <span class="sep"></span>
      <span class="seg-label">Servicios</span>
      {#each services as s, i (s.id)}
        {@const fav = faviconUrl(s.url)}
        {@const useFav = fav && !faviconFailed.has("svc:" + s.id)}
        <div class="svc-group">
          <button class="ord-btn" onclick={() => moveService(s.id, -1)} disabled={i === 0} title="Subir">▲</button>
          <button class="pick" onclick={() => startEdit(s)} title="Editar {s.name}">
            <span class="pick-icon" class:img={useFav} style:background={useFav ? "transparent" : (s.color ?? "#444")}>
              {#if useFav}
                <img src={fav} alt="" onerror={() => markFaviconFailed("svc:" + s.id)} />
              {:else}
                {initialOf(s)}
              {/if}
            </span>
            <span class="pick-name">{s.name}</span>
          </button>
          <button class="ord-btn" onclick={() => moveService(s.id, 1)} disabled={i === services.length - 1} title="Bajar">▼</button>
        </div>
      {/each}
      <span class="sep"></span>
      <span class="seg-label">Sistema</span>
      <button class="seg" class:on={autostartOn} onclick={toggleAutostart} title="Iniciar con sesión">
        Autostart {autostartOn ? "ON" : "OFF"}
      </button>
      <span class="sep"></span>
      <span class="seg-label">Atajo global</span>
      <input
        type="text"
        class="kbd-input"
        bind:value={shortcutDraft}
        onkeydown={captureShortcut}
        data-no-drag
        placeholder="Ctrl+Alt+K"
        title="Click y presioná la combinación"
      />
      <button class="seg" onclick={saveShortcut}>Guardar</button>
      {#if shortcutMsg}<span class="kbd-item">{shortcutMsg}</span>{/if}
      <span class="sep"></span>
      <button class="seg" onclick={() => (mode = "shortcuts")}>Atajos bar</button>
    </div>
  {:else if mode === "edit"}
    <button class="add cancel" onclick={cancelEdit} title="Cancelar">×</button>
    <form class="custom-form" onsubmit={(e) => { e.preventDefault(); saveEdit(); }}>
      <input type="text" placeholder="Nombre" bind:value={editName} data-no-drag required />
      <input type="text" placeholder="URL" bind:value={editUrl} data-no-drag required />
      <input type="text" placeholder="Inicial/emoji" bind:value={editIcon} data-no-drag class="icon-input" maxlength="2" title="Icono custom (1-2 chars/emoji)" />
      <input type="color" bind:value={editColor} data-no-drag class="color-input" title="Color" />
      <button type="button" class="link-btn" onclick={() => (editColor = "")}>limpiar</button>
      <button type="submit" class="custom-go">Guardar</button>
    </form>
  {:else if mode === "shortcuts"}
    <button class="add cancel" onclick={() => (mode = "settings")} title="Volver">×</button>
    <div class="picker shortcuts-row">
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>1</kbd>..<kbd>9</kbd> tab N</span>
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>T</kbd> agregar</span>
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>W</kbd> eliminar actual</span>
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>0</kbd>/<kbd>H</kbd> inicio</span>
    </div>
  {/if}

  <div class="spacer" data-tauri-drag-region></div>

  <div class="controls">
    <button class="ctrl" onclick={showHome} title="Inicio">⌂</button>
    {#if activeId}
      <button class="ctrl" onclick={reloadActive} title="Recargar">↻</button>
    {/if}
    <button class="ctrl" onclick={openSettings} title="Configuración">⚙</button>
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
    <img class="logo" src="/logo.png" alt="Kolibri" draggable="false" />
    <h1>Bienvenido a Kolibri</h1>
    <p>Click ＋ arriba para agregar tu primer servicio</p>
  {:else if !activeId}
    <img class="logo" src="/logo.png" alt="Kolibri" draggable="false" />
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

  .seg-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: #888;
    padding: 0 4px;
    flex-shrink: 0;
  }
  .seg {
    height: 30px;
    padding: 0 12px;
    background: #2a2a2a;
    border: 1px solid #3a3a3a;
    border-radius: 6px;
    color: #ddd;
    cursor: pointer;
    font-size: 12px;
    flex-shrink: 0;
  }
  .seg:hover { background: #383838; }
  .seg.on { background: #4a8; border-color: #5b9; color: #fff; }
  .sep {
    width: 1px;
    height: 22px;
    background: #3a3a3a;
    flex-shrink: 0;
    margin: 0 4px;
  }
  .svc-group {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }
  .ord-btn {
    background: transparent;
    border: 1px solid #3a3a3a;
    color: #aaa;
    width: 20px;
    height: 30px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 9px;
    padding: 0;
  }
  .ord-btn:disabled { opacity: 0.3; cursor: default; }
  .ord-btn:not(:disabled):hover { background: #333; color: #fff; }
  .color-input {
    width: 32px;
    height: 30px;
    background: transparent;
    border: 1px solid #3a3a3a;
    border-radius: 4px;
    cursor: pointer;
    padding: 0;
  }
  .link-btn {
    background: transparent;
    border: none;
    color: #6ab;
    cursor: pointer;
    font-size: 11px;
    padding: 0 4px;
  }
  .link-btn:hover { color: #8cd; text-decoration: underline; }
  .shortcuts-row { gap: 14px; }
  .kbd-item { font-size: 11px; color: #bbb; flex-shrink: 0; white-space: nowrap; }
  kbd {
    background: #2a2a2a;
    border: 1px solid #3a3a3a;
    border-radius: 3px;
    padding: 1px 5px;
    font-family: monospace;
    font-size: 10px;
    color: #ddd;
  }

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
  .welcome .logo {
    width: min(40vw, 320px);
    height: auto;
    object-fit: contain;
    -webkit-user-drag: none;
    user-select: none;
    filter: drop-shadow(0 8px 24px rgba(0,0,0,0.4));
  }

  .rz { position: fixed; z-index: 9999; background: transparent; }
  .rz-s  { bottom: 0; left: 8px; right: 8px; height: 4px; cursor: s-resize; }
  .rz-e  { top: 56px; bottom: 8px; right: 0; width: 4px; cursor: e-resize; }
  .rz-w  { top: 56px; bottom: 8px; left: 0; width: 4px; cursor: w-resize; }
  .rz-se { bottom: 0; right: 0; width: 8px; height: 8px; cursor: se-resize; }
  .rz-sw { bottom: 0; left: 0; width: 8px; height: 8px; cursor: sw-resize; }

  .icon-input {
    width: 70px;
    text-align: center;
  }
  .kbd-input {
    background: #1d1d1d;
    border: 1px solid #3a3a3a;
    border-radius: 6px;
    padding: 0 10px;
    height: 30px;
    color: #eee;
    font-size: 12px;
    outline: none;
    width: 140px;
    font-family: monospace;
  }
  .kbd-input:focus { border-color: #6a8; }

  .badge {
    background: #e44;
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 8px;
    min-width: 16px;
    text-align: center;
    line-height: 1.4;
    margin-left: 2px;
    box-shadow: 0 0 0 1px rgba(0,0,0,0.4);
  }
</style>

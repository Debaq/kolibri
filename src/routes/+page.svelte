<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { CATALOG, type ServiceTemplate } from "$lib/catalog";
  import { isEnabled as autostartIsEnabled, enable as autostartEnable, disable as autostartDisable } from "@tauri-apps/plugin-autostart";
  import { openUrl } from "@tauri-apps/plugin-opener";

  type Theme = "dark" | "light" | "auto";

  type Service = {
    id: string;
    name: string;
    url: string;
    icon: string | null;
    color: string | null;
    session_slot?: number;
    keep_alive?: boolean;
  };

  type Mode = "tabs" | "picker" | "custom" | "remove" | "settings" | "edit" | "shortcuts" | "reorder";

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
  let editKeepAlive = $state(false);
  let loadingIds = $state<Set<string>>(new Set());
  let unread = $state<Record<string, number>>({});
  let unreadTotal = $state(0);
  let autostartOn = $state(false);
  let toggleShortcut = $state("Ctrl+Alt+K");
  let shortcutDraft = $state("");
  let shortcutMsg = $state("");
  let memBytes = $state(0);
  let memTimer: ReturnType<typeof setInterval> | null = null;

  type UpdateInfo = { available: boolean; current: string; latest: string; url: string; repo: string };
  let updateInfo = $state<UpdateInfo | null>(null);
  let updateTimer: ReturnType<typeof setInterval> | null = null;
  const UPDATE_DISMISS_KEY = "kolibri:update_dismissed";

  // Auto-suspend de pestañas inactivas
  let suspendMinutes = $state<number>(5);
  let suspendDraft = $state<number>(5);
  let suspendMsg = $state("");
  // Por servicio: timer pendiente para llamar suspend_service
  const suspendTimers = new Map<string, ReturnType<typeof setTimeout>>();
  // Servicios actualmente "suspendidos" en el front (para mostrar UI o evitar dobles).
  let suspendedIds = $state<Set<string>>(new Set());
  // Aviso de migración de sesiones
  let migrationNotice = $state(false);

  function clearSuspendTimer(id: string) {
    const t = suspendTimers.get(id);
    if (t) {
      clearTimeout(t);
      suspendTimers.delete(id);
    }
  }

  function scheduleSuspend(id: string) {
    clearSuspendTimer(id);
    if (!suspendMinutes || suspendMinutes <= 0) return;
    const svc = services.find((s) => s.id === id);
    if (!svc || svc.keep_alive) return;
    const ms = suspendMinutes * 60 * 1000;
    const t = setTimeout(async () => {
      try {
        const ok = await invoke<boolean>("suspend_service", { id });
        if (ok) {
          const next = new Set(suspendedIds);
          next.add(id);
          suspendedIds = next;
        }
      } catch {}
      suspendTimers.delete(id);
    }, ms);
    suspendTimers.set(id, t);
  }

  function rescheduleAllSuspends() {
    // Limpia y reprograma todos los timers para no-activos sin keep_alive.
    for (const id of Array.from(suspendTimers.keys())) clearSuspendTimer(id);
    if (!suspendMinutes || suspendMinutes <= 0) return;
    for (const s of services) {
      if (s.id === activeId) continue;
      if (s.keep_alive) continue;
      if (suspendedIds.has(s.id)) continue;
      scheduleSuspend(s.id);
    }
  }

  async function loadSuspendMinutes() {
    try {
      suspendMinutes = await invoke<number>("get_inactive_suspend_minutes");
      suspendDraft = suspendMinutes;
    } catch {}
  }

  async function saveSuspendMinutes() {
    const n = Math.max(0, Math.floor(Number(suspendDraft) || 0));
    try {
      await invoke("set_inactive_suspend_minutes", { minutes: n });
      suspendMinutes = n;
      suspendMsg = "OK";
      rescheduleAllSuspends();
    } catch (e) {
      suspendMsg = "Error: " + e;
    }
    setTimeout(() => (suspendMsg = ""), 2000);
  }

  function dismissMigrationNotice() {
    migrationNotice = false;
  }

  async function checkUpdate() {
    try {
      const info = await invoke<UpdateInfo>("check_update");
      if (!info.available) { updateInfo = null; return; }
      const dismissed = typeof localStorage !== "undefined" ? localStorage.getItem(UPDATE_DISMISS_KEY) : null;
      if (dismissed && dismissed === info.latest) { updateInfo = null; return; }
      updateInfo = info;
    } catch {}
  }

  async function openUpdate() {
    if (!updateInfo) return;
    try { await openUrl(updateInfo.url); } catch {}
  }

  function dismissUpdate() {
    if (updateInfo && typeof localStorage !== "undefined") {
      localStorage.setItem(UPDATE_DISMISS_KEY, updateInfo.latest);
    }
    updateInfo = null;
  }

  function formatMem(b: number): string {
    if (!b) return "—";
    const mb = b / (1024 * 1024);
    if (mb >= 1024) return (mb / 1024).toFixed(2) + " GB";
    return mb.toFixed(0) + " MB";
  }

  async function refreshMem() {
    try {
      memBytes = await invoke<number>("get_memory_usage");
    } catch {}
  }

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

  let systemDarkMql: MediaQueryList | null = null;
  function resolvedTheme(t: Theme): "dark" | "light" {
    if (t !== "auto") return t;
    if (typeof window === "undefined") return "dark";
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  function onSystemThemeChange() {
    if (theme === "auto" && typeof document !== "undefined") {
      document.documentElement.dataset.theme = resolvedTheme("auto");
    }
  }

  function applyTheme(t: Theme) {
    theme = t;
    if (typeof document !== "undefined") {
      document.documentElement.dataset.theme = resolvedTheme(t);
    }
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("kolibri:theme", t);
    }
    if (typeof window !== "undefined") {
      if (!systemDarkMql) {
        systemDarkMql = window.matchMedia("(prefers-color-scheme: dark)");
        systemDarkMql.addEventListener("change", onSystemThemeChange);
      }
    }
  }
  let unlisteners: UnlistenFn[] = [];

  let favCache = $state<Record<string, string>>({});
  const favLoading = new Set<string>();

  function hostOf(url: string): string | null {
    try {
      return new URL(url).hostname.toLowerCase();
    } catch {
      return null;
    }
  }

  function faviconUrl(url: string): string | null {
    const host = hostOf(url);
    if (!host) return null;
    return favCache[host] ?? null;
  }

  async function loadFavicon(url: string) {
    const host = hostOf(url);
    if (!host) return;
    if (favCache[host] || favLoading.has(host)) return;
    favLoading.add(host);
    try {
      const data = await invoke<string>("get_favicon", { url });
      favCache = { ...favCache, [host]: data };
    } catch {
      markFaviconFailed("host:" + host);
    } finally {
      favLoading.delete(host);
    }
  }

  function markFaviconFailed(key: string) {
    if (faviconFailed.has(key)) return;
    const next = new Set(faviconFailed);
    next.add(key);
    faviconFailed = next;
  }

  $effect(() => {
    for (const s of services) loadFavicon(s.url);
  });
  $effect(() => {
    if (mode === "picker") for (const t of CATALOG) loadFavicon(t.url);
  });

  async function refresh() {
    services = await invoke<Service[]>("list_services");
    activeId = await invoke<string | null>("get_active_service");
    rescheduleAllSuspends();
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
    // Si estaba marcada como suspendida en el front, ya no lo está (backend la remontará).
    if (suspendedIds.has(id)) {
      const next = new Set(suspendedIds);
      next.delete(id);
      suspendedIds = next;
    }
    // Pausar timer de la pestaña que estamos por activar (no se debe suspender mientras está activa).
    clearSuspendTimer(id);
    // Si había una pestaña activa anterior, programar su suspensión.
    const prev = activeId;
    await invoke("switch_service", { id });
    activeId = id;
    if (prev && prev !== id) scheduleSuspend(prev);
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
    editKeepAlive = !!s.keep_alive;
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
      keepAlive: editKeepAlive,
    });
    editingId = null;
    mode = "settings";
    await refresh();
  }

  function cancelEdit() {
    editingId = null;
    mode = "settings";
  }

  let desktopMsg = $state("");
  async function reinstallDesktop() {
    try {
      const ok = await invoke<boolean>("reinstall_desktop_entry");
      desktopMsg = ok ? "Reinstalado" : "No aplica";
    } catch (e) {
      desktopMsg = "Error: " + e;
    }
    setTimeout(() => (desktopMsg = ""), 2500);
  }

  let clearMsg = $state("");
  async function clearSession(label: string) {
    if (!editingId) return;
    const svc = services.find(s => s.id === editingId);
    if (!svc) return;
    const shared = services.filter(s => s.id !== svc.id && hostOf(s.url) === hostOf(svc.url) && (s.session_slot ?? 0) === (svc.session_slot ?? 0));
    const extra = shared.length ? `\n\nTambién afecta: ${shared.map(s => s.name).join(", ")} (comparten sesión).` : "";
    if (!confirm(`${label} de "${svc.name}"?${extra}`)) return;
    try {
      await invoke("clear_service_session", { id: editingId });
      clearMsg = "Listo";
      setTimeout(() => (clearMsg = ""), 2000);
    } catch (e) {
      clearMsg = String(e);
    }
  }

  async function renameService(s: Service, name: string) {
    const trimmed = name.trim();
    if (!trimmed || trimmed === s.name) return;
    await invoke("update_service", {
      id: s.id,
      name: trimmed,
      url: s.url,
      icon: s.icon ?? "",
      color: s.color ?? "",
      keepAlive: s.keep_alive ?? false,
    });
    await refresh();
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
    try {
      await loadSuspendMinutes();
    } catch {}
    unlisteners.push(await listen("kolibri:sessions_migrated", () => {
      migrationNotice = true;
    }));
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
          const { service_id, count, total } = e.payload;
          const next = { ...unread };
          if (count > 0) next[service_id] = count;
          else delete next[service_id];
          unread = next;
          unreadTotal = total;
        }
      )
    );
    window.addEventListener("keydown", onKey);
    refreshMem();
    memTimer = setInterval(refreshMem, 2000);
    checkUpdate();
    updateTimer = setInterval(checkUpdate, 6 * 60 * 60 * 1000);
  });

  onDestroy(() => {
    unlisteners.forEach((u) => u());
    window.removeEventListener("keydown", onKey);
    if (memTimer) clearInterval(memTimer);
    if (updateTimer) clearInterval(updateTimer);
    for (const id of Array.from(suspendTimers.keys())) clearSuspendTimer(id);
  });
</script>

<div class="rz rz-s" role="separator" aria-orientation="horizontal" aria-label="Redimensionar abajo" onmousedown={startResize("South")}></div>
<div class="rz rz-e" role="separator" aria-orientation="vertical" aria-label="Redimensionar derecha" onmousedown={startResize("East")}></div>
<div class="rz rz-w" role="separator" aria-orientation="vertical" aria-label="Redimensionar izquierda" onmousedown={startResize("West")}></div>
<div class="rz rz-se" role="separator" aria-label="Redimensionar esquina inferior derecha" onmousedown={startResize("SouthEast")}></div>
<div class="rz rz-sw" role="separator" aria-label="Redimensionar esquina inferior izquierda" onmousedown={startResize("SouthWest")}></div>

<div class="bar" role="toolbar" aria-label="Barra de servicios" data-tauri-drag-region onmousedown={dragOn}>
  {#if mode === "tabs"}
    <button class="add" onclick={startPicker} title="Agregar servicio">＋</button>
    {#if unreadTotal > 0}
      <span class="badge total" title="Total sin leer">{unreadTotal > 99 ? "99+" : unreadTotal}</span>
    {/if}

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
      <button class="seg" class:on={theme === "auto"} onclick={() => applyTheme("auto")} title="Seguir tema del sistema">Auto</button>
      <span class="sep"></span>
      <span class="seg-label">Servicios</span>
      {#each services as s (s.id)}
        {@const fav = faviconUrl(s.url)}
        {@const useFav = fav && !faviconFailed.has("svc:" + s.id)}
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
      {/each}
      {#if services.length > 1}
        <button class="seg" onclick={() => (mode = "reorder")} title="Ordenar servicios">Ordenar</button>
      {/if}
      <span class="sep"></span>
      <span class="seg-label">Sistema</span>
      <button class="seg" class:on={autostartOn} onclick={toggleAutostart} title="Iniciar con sesión">
        Autostart {autostartOn ? "ON" : "OFF"}
      </button>
      <button class="seg" onclick={reinstallDesktop} title="Reinstalar entrada en menu/launcher (Linux)">
        {desktopMsg || "Reinstalar entrada de menu"}
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
      <span class="seg-label" title="Suspende pestañas en segundo plano para liberar RAM. 0 = nunca.">Suspender inactivas (min)</span>
      <input
        type="number"
        class="kbd-input"
        min="0"
        max="1440"
        bind:value={suspendDraft}
        data-no-drag
        title="Minutos de inactividad para suspender (0 = nunca)"
        style="width: 70px"
      />
      <button class="seg" onclick={saveSuspendMinutes}>Guardar</button>
      {#if suspendMsg}<span class="kbd-item">{suspendMsg}</span>{/if}
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
      <label class="kbd-item" data-no-drag title="Si está activo, no se suspende por inactividad (útil para apps con notificaciones en background)">
        <input type="checkbox" bind:checked={editKeepAlive} data-no-drag />
        Mantener viva en segundo plano
      </label>
      <button type="submit" class="custom-go">Guardar</button>
      <span class="sep"></span>
      <button type="button" class="seg" data-no-drag onclick={() => clearSession("Limpiar cookies y cache")} title="Borra cookies/cache; mantiene el servicio en la lista">Limpiar cookies</button>
      <button type="button" class="seg" data-no-drag onclick={() => clearSession("Cerrar sesión")} title="Cierra sesión borrando cookies; el servicio queda en la lista">Cerrar sesión</button>
      {#if clearMsg}<span class="kbd-item">{clearMsg}</span>{/if}
    </form>
  {:else if mode === "reorder"}
    <button class="add cancel" onclick={() => (mode = "settings")} title="Volver">×</button>
    <div class="picker">
      <span class="seg-label">Ordenar</span>
      {#each services as s, i (s.id)}
        {@const fav = faviconUrl(s.url)}
        {@const useFav = fav && !faviconFailed.has("svc:" + s.id)}
        <div class="svc-group">
          <button class="ord-btn" onclick={() => moveService(s.id, -1)} disabled={i === 0} title="Subir">▲</button>
          <span class="pick" title={s.name}>
            <span class="pick-icon" class:img={useFav} style:background={useFav ? "transparent" : (s.color ?? "#444")}>
              {#if useFav}
                <img src={fav} alt="" onerror={() => markFaviconFailed("svc:" + s.id)} />
              {:else}
                {initialOf(s)}
              {/if}
            </span>
            <input
              type="text"
              class="pick-name pick-name-input"
              value={s.name}
              data-no-drag
              onchange={(e) => renameService(s, (e.currentTarget as HTMLInputElement).value)}
              onkeydown={(e) => { if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur(); }}
              title="Renombrar"
            />
          </span>
          <button class="ord-btn" onclick={() => moveService(s.id, 1)} disabled={i === services.length - 1} title="Bajar">▼</button>
        </div>
      {/each}
    </div>
  {:else if mode === "shortcuts"}
    <button class="add cancel" onclick={() => (mode = "settings")} title="Volver">×</button>
    <div class="picker shortcuts-row">
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>1</kbd>..<kbd>9</kbd> tab N</span>
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>T</kbd> agregar</span>
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>W</kbd> eliminar actual</span>
      <span class="kbd-item"><kbd>Ctrl</kbd>+<kbd>0</kbd>/<kbd>H</kbd> inicio</span>
    </div>
  {/if}

  {#if mode === "tabs"}
    <div class="spacer" data-tauri-drag-region></div>
  {/if}

  <div class="controls">
    {#if updateInfo}
      <button
        class="update-badge"
        onclick={openUpdate}
        oncontextmenu={(e) => { e.preventDefault(); dismissUpdate(); }}
        title={`Nueva versión ${updateInfo.latest} disponible (actual ${updateInfo.current}). Click: abrir release. Click derecho: descartar.`}
        data-no-drag
      >⬆ {updateInfo.latest}</button>
    {/if}
    <span class="mem" title="RAM en uso (proceso + webviews)">{formatMem(memBytes)}</span>
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

{#if migrationNotice}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="migr-title">
    <div class="modal">
      <h2 id="migr-title">Sesiones reseteadas</h2>
      <p>
        Esta versión cambia cómo se almacenan las sesiones para reducir el uso de RAM.
        Servicios distintos del mismo origen ahora comparten un único proceso de WebKit.
      </p>
      <p>
        Como efecto secundario, las sesiones anteriores fueron eliminadas. Tendrás que volver
        a iniciar sesión en cada servicio.
      </p>
      <div class="modal-actions">
        <button class="custom-go" onclick={dismissMigrationNotice}>Entendido</button>
      </div>
    </div>
  </div>
{/if}

<!-- El área debajo del bar la dibuja un widget GTK nativo (logo) en
     `webview/layout_linux.rs`. No renderizar nada acá para no tapar el bar. -->

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
  .pick-name-input {
    background: transparent;
    border: 1px solid transparent;
    color: inherit;
    font: inherit;
    padding: 2px 4px;
    border-radius: 4px;
    width: 120px;
    outline: none;
  }
  .pick-name-input:hover { border-color: #555; }
  .pick-name-input:focus { border-color: #888; background: #1f1f1f; }

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
  .mem {
    font-size: 11px;
    color: #aaa;
    padding: 0 8px;
    font-variant-numeric: tabular-nums;
    user-select: none;
    white-space: nowrap;
  }
  :global([data-theme="light"]) .mem { color: #555; }
  .update-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    margin: 0 6px;
    padding: 0 10px;
    border: 1px solid #00b894;
    border-radius: 11px;
    background: rgba(0, 184, 148, 0.12);
    color: #00b894;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    line-height: 1;
  }
  .update-badge:hover { background: rgba(0, 184, 148, 0.25); color: #fff; }
  :global([data-theme="light"]) .update-badge { color: #007a63; background: rgba(0, 184, 148, 0.15); }
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
  .badge.total {
    margin: 0 6px 0 2px;
    font-size: 11px;
    padding: 2px 7px;
    align-self: center;
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9999;
  }
  .modal {
    background: var(--kolibri-bar, #222);
    color: var(--kolibri-fg, #eee);
    border: 1px solid var(--kolibri-bar-border, #2c2c2c);
    border-radius: 10px;
    padding: 22px 26px;
    max-width: 460px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  .modal h2 {
    margin: 0 0 12px 0;
    font-size: 18px;
  }
  .modal p {
    margin: 6px 0;
    font-size: 13px;
    line-height: 1.45;
    opacity: 0.92;
  }
  .modal-actions {
    margin-top: 16px;
    display: flex;
    justify-content: flex-end;
  }
</style>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type Service = {
    id: string;
    name: string;
    url: string;
    icon: string | null;
    color: string | null;
  };

  type Theme = "dark" | "light";

  let { services, theme, onClose, onChanged, onTheme }: {
    services: Service[];
    theme: Theme;
    onClose: () => void;
    onChanged: () => Promise<void> | void;
    onTheme: (t: Theme) => void;
  } = $props();

  let editingId = $state<string | null>(null);
  let editName = $state("");
  let editUrl = $state("");
  let editColor = $state("");

  function startEdit(s: Service) {
    editingId = s.id;
    editName = s.name;
    editUrl = s.url;
    editColor = s.color ?? "";
  }

  function cancelEdit() {
    editingId = null;
  }

  async function saveEdit() {
    if (!editingId) return;
    await invoke("update_service", {
      id: editingId,
      name: editName.trim(),
      url: editUrl.trim(),
      icon: null,
      color: editColor,
    });
    editingId = null;
    await onChanged();
  }

  async function move(id: string, delta: number) {
    const ids = services.map((s) => s.id);
    const i = ids.indexOf(id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    await invoke("reorder_services", { ids });
    await onChanged();
  }

  function setTheme(t: Theme) {
    onTheme(t);
  }
</script>

<div class="overlay" onclick={onClose} role="presentation"></div>
<aside class="panel" aria-label="Configuración">
  <header>
    <h2>Configuración</h2>
    <button class="x" onclick={onClose} title="Cerrar">×</button>
  </header>

  <section>
    <h3>Tema</h3>
    <div class="row">
      <button class:on={theme === "dark"} onclick={() => setTheme("dark")}>Oscuro</button>
      <button class:on={theme === "light"} onclick={() => setTheme("light")}>Claro</button>
    </div>
  </section>

  <section>
    <h3>Servicios</h3>
    {#if services.length === 0}
      <p class="muted">Sin servicios.</p>
    {/if}
    <ul>
      {#each services as s, i (s.id)}
        <li>
          {#if editingId === s.id}
            <div class="edit">
              <label>Nombre <input bind:value={editName} /></label>
              <label>URL <input bind:value={editUrl} /></label>
              <label>Color
                <input type="color" bind:value={editColor} />
                <button class="link" onclick={() => (editColor = "")}>limpiar</button>
              </label>
              <div class="actions">
                <button class="primary" onclick={saveEdit}>Guardar</button>
                <button onclick={cancelEdit}>Cancelar</button>
              </div>
            </div>
          {:else}
            <div class="item">
              <span class="dot" style:background={s.color ?? "#444"}></span>
              <span class="name">{s.name}</span>
              <span class="url">{s.url}</span>
              <span class="ord">
                <button onclick={() => move(s.id, -1)} disabled={i === 0} title="Subir">▲</button>
                <button onclick={() => move(s.id, 1)} disabled={i === services.length - 1} title="Bajar">▼</button>
              </span>
              <button class="link" onclick={() => startEdit(s)}>Editar</button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </section>

  <section>
    <h3>Atajos</h3>
    <ul class="kbd">
      <li><kbd>Ctrl</kbd>+<kbd>1</kbd>..<kbd>9</kbd> — cambiar a tab N</li>
      <li><kbd>Ctrl</kbd>+<kbd>T</kbd> — agregar servicio</li>
      <li><kbd>Ctrl</kbd>+<kbd>W</kbd> — eliminar servicio actual</li>
      <li><kbd>Ctrl</kbd>+<kbd>0</kbd> / <kbd>H</kbd> — ir a inicio</li>
    </ul>
    <p class="muted">Funciona cuando la barra tiene foco. Atajos globales requieren plugin global-shortcut (pendiente).</p>
  </section>
</aside>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    z-index: 50;
  }
  .panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: 420px;
    max-width: 100vw;
    background: #1d1d1d;
    color: #ddd;
    border-left: 1px solid #333;
    z-index: 51;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    font-size: 13px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid #2c2c2c;
    position: sticky;
    top: 0;
    background: #1d1d1d;
  }
  header h2 { margin: 0; font-size: 16px; font-weight: 500; }
  .x {
    background: transparent;
    border: none;
    color: #aaa;
    font-size: 22px;
    cursor: pointer;
    line-height: 1;
  }
  .x:hover { color: #fff; }
  section { padding: 14px 16px; border-bottom: 1px solid #2c2c2c; }
  section h3 { margin: 0 0 10px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.5px; color: #888; }
  .muted { color: #888; font-size: 12px; }
  .row { display: flex; gap: 8px; }
  .row button, .actions button {
    background: #2a2a2a;
    border: 1px solid #3a3a3a;
    color: #ddd;
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
  }
  .row button.on { background: #4a8; border-color: #5b9; color: #fff; }
  .row button:hover { background: #383838; }
  .actions button.primary { background: #4a8; border-color: #5b9; color: #fff; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li { background: #232323; border: 1px solid #2c2c2c; border-radius: 6px; padding: 8px 10px; }
  .item { display: grid; grid-template-columns: 16px 1fr auto auto; align-items: center; gap: 8px; }
  .item .name { font-weight: 500; }
  .item .url {
    grid-column: 2 / 3;
    grid-row: 2;
    font-size: 11px;
    color: #888;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot { width: 12px; height: 12px; border-radius: 3px; grid-row: span 2; }
  .ord { display: flex; gap: 2px; grid-row: span 2; }
  .ord button {
    background: transparent;
    border: 1px solid #3a3a3a;
    color: #aaa;
    width: 22px;
    height: 22px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 10px;
    padding: 0;
  }
  .ord button:disabled { opacity: 0.3; cursor: default; }
  .ord button:not(:disabled):hover { background: #333; color: #fff; }
  .link {
    background: transparent;
    border: none;
    color: #6ab;
    cursor: pointer;
    font-size: 12px;
    padding: 2px 6px;
    grid-row: span 2;
  }
  .link:hover { color: #8cd; text-decoration: underline; }
  .edit { display: flex; flex-direction: column; gap: 8px; }
  .edit label { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: #aaa; }
  .edit input:not([type="color"]) {
    background: #1a1a1a;
    border: 1px solid #3a3a3a;
    color: #eee;
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 12px;
  }
  .edit input[type="color"] {
    width: 40px;
    height: 24px;
    background: transparent;
    border: 1px solid #3a3a3a;
    border-radius: 4px;
    cursor: pointer;
    padding: 0;
  }
  .actions { display: flex; gap: 8px; }
  .kbd { font-size: 12px; }
  .kbd li { background: transparent; border: none; padding: 2px 0; }
  kbd {
    background: #2a2a2a;
    border: 1px solid #3a3a3a;
    border-radius: 3px;
    padding: 1px 5px;
    font-family: monospace;
    font-size: 11px;
  }
</style>

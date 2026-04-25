<script lang="ts">
  import { CATALOG, type ServiceTemplate } from "./catalog";

  type Props = {
    onclose: () => void;
    onpick: (svc: { name: string; url: string; icon: string | null; color?: string }) => void;
  };
  let { onclose, onpick }: Props = $props();

  let mode = $state<"catalog" | "custom">("catalog");
  let query = $state("");

  let customName = $state("");
  let customUrl = $state("");

  const filtered = $derived(
    CATALOG.filter((s) =>
      s.name.toLowerCase().includes(query.toLowerCase()) ||
      s.description.toLowerCase().includes(query.toLowerCase()),
    ),
  );

  function pickTemplate(t: ServiceTemplate) {
    onpick({ name: t.name, url: t.url, icon: t.initial, color: t.color });
  }

  function submitCustom(e: Event) {
    e.preventDefault();
    if (!customName.trim() || !customUrl.trim()) return;
    let url = customUrl.trim();
    if (!/^https?:\/\//i.test(url)) url = "https://" + url;
    onpick({ name: customName.trim(), url, icon: null });
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onclose();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="backdrop" onclick={onBackdropClick} role="presentation">
  <div class="modal" role="dialog" aria-modal="true" aria-label="Agregar servicio">
    <header>
      <h2>Agregar servicio</h2>
      <button class="close" onclick={onclose} aria-label="Cerrar">×</button>
    </header>

    <nav class="tabs">
      <button class:active={mode === "catalog"} onclick={() => (mode = "catalog")}>Catálogo</button>
      <button class:active={mode === "custom"} onclick={() => (mode = "custom")}>Personalizado</button>
    </nav>

    {#if mode === "catalog"}
      <input
        class="search"
        type="search"
        placeholder="Buscar servicio..."
        bind:value={query}
        autofocus
      />
      <div class="grid">
        {#each filtered as t (t.key)}
          <button class="card" onclick={() => pickTemplate(t)}>
            <span class="icon" style:background={t.color}>{t.initial}</span>
            <span class="meta">
              <span class="name">{t.name}</span>
              <span class="desc">{t.description}</span>
            </span>
          </button>
        {/each}
        {#if filtered.length === 0}
          <p class="empty-grid">Nada encontrado. Prueba "Personalizado".</p>
        {/if}
      </div>
    {:else}
      <form class="custom" onsubmit={submitCustom}>
        <label>
          Nombre
          <input type="text" bind:value={customName} placeholder="Mi servicio" autofocus required />
        </label>
        <label>
          URL
          <input type="text" bind:value={customUrl} placeholder="https://ejemplo.com" required />
        </label>
        <button type="submit" class="primary">Agregar</button>
      </form>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .modal {
    background: #2a2a2a;
    border: 1px solid #3a3a3a;
    border-radius: 12px;
    width: min(560px, 92vw);
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid #3a3a3a;
  }
  header h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .close {
    background: transparent;
    border: none;
    color: #aaa;
    font-size: 24px;
    cursor: pointer;
    line-height: 1;
    padding: 0 4px;
  }
  .close:hover { color: #fff; }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 8px 12px 0;
  }
  .tabs button {
    background: transparent;
    border: none;
    color: #999;
    padding: 8px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .tabs button:hover { background: #333; color: #ddd; }
  .tabs button.active { background: #383838; color: #fff; }
  .search {
    margin: 12px 16px 0;
    padding: 9px 12px;
    background: #1d1d1d;
    border: 1px solid #3a3a3a;
    border-radius: 6px;
    color: #eee;
    font-size: 13px;
    outline: none;
  }
  .search:focus { border-color: #555; }
  .grid {
    overflow-y: auto;
    padding: 12px 16px 16px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: #333;
    border: 1px solid #3a3a3a;
    border-radius: 8px;
    color: #eee;
    cursor: pointer;
    text-align: left;
  }
  .card:hover { background: #3a3a3a; border-color: #555; }
  .icon {
    width: 38px;
    height: 38px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: white;
    font-weight: 700;
    flex-shrink: 0;
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name { font-weight: 600; font-size: 13px; }
  .desc { font-size: 11px; color: #999; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .empty-grid {
    grid-column: 1 / -1;
    text-align: center;
    color: #777;
    padding: 20px;
  }
  .custom {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }
  .custom label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: #aaa;
  }
  .custom input {
    background: #1d1d1d;
    border: 1px solid #3a3a3a;
    border-radius: 6px;
    padding: 9px 12px;
    color: #eee;
    font-size: 13px;
    outline: none;
  }
  .custom input:focus { border-color: #555; }
  .primary {
    background: #4a8;
    color: #fff;
    border: none;
    padding: 10px;
    border-radius: 6px;
    font-weight: 600;
    cursor: pointer;
    margin-top: 4px;
  }
  .primary:hover { background: #5b9; }
</style>

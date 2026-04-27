<script lang="ts">
  import { onDestroy } from "svelte";
  import { mailApi, formatDate, type MailHeader, type MailMessage } from "./mail";

  type Props = { serviceId: string };
  let { serviceId }: Props = $props();

  let headers = $state<MailHeader[]>([]);
  let selected = $state<MailMessage | null>(null);
  let selectedId = $state<string | null>(null);
  let loadingList = $state(false);
  let loadingMsg = $state(false);
  let error = $state<string | null>(null);

  let lastServiceId = "";

  $effect(() => {
    if (serviceId !== lastServiceId) {
      lastServiceId = serviceId;
      headers = [];
      selected = null;
      selectedId = null;
      error = null;
      loadInbox();
    }
  });

  async function loadInbox() {
    loadingList = true;
    error = null;
    try {
      headers = await mailApi.listInbox(serviceId, 0, 50);
    } catch (e: any) {
      error = String(e);
    } finally {
      loadingList = false;
    }
  }

  async function open(h: MailHeader) {
    selectedId = h.id;
    selected = null;
    loadingMsg = true;
    try {
      selected = await mailApi.getMessage(serviceId, h.id);
    } catch (e: any) {
      error = String(e);
    } finally {
      loadingMsg = false;
    }
  }

  onDestroy(() => {
    headers = [];
    selected = null;
  });
</script>

<div class="mail">
  <header class="mail-head">
    <button onclick={loadInbox} disabled={loadingList} title="Recargar">↻</button>
    <span class="count">{headers.length} mensajes</span>
    {#if error}<span class="err">{error}</span>{/if}
  </header>

  <div class="split">
    <ul class="list">
      {#if loadingList && headers.length === 0}
        <li class="empty">Cargando…</li>
      {:else if headers.length === 0}
        <li class="empty">Bandeja vacía</li>
      {/if}
      {#each headers as h (h.id)}
        <li
          class="row"
          class:active={selectedId === h.id}
          class:unseen={!h.seen}
          onclick={() => open(h)}
        >
          <div class="row-top">
            <span class="from">{h.from_name || h.from_addr}</span>
            <span class="date">{formatDate(h.date_ts)}</span>
          </div>
          <div class="subject">{h.subject || "(sin asunto)"}</div>
        </li>
      {/each}
    </ul>

    <section class="reader">
      {#if loadingMsg}
        <div class="placeholder">Cargando mensaje…</div>
      {:else if selected}
        <header class="reader-head">
          <h2>{selected.subject || "(sin asunto)"}</h2>
          <div class="meta">
            <div><b>De:</b> {selected.from_name} &lt;{selected.from_addr}&gt;</div>
            {#if selected.to.length}
              <div><b>Para:</b> {selected.to.join(", ")}</div>
            {/if}
            {#if selected.cc.length}
              <div><b>Cc:</b> {selected.cc.join(", ")}</div>
            {/if}
            <div class="date">{formatDate(selected.date_ts)}</div>
          </div>
        </header>
        <div class="body">
          {#if selected.body_html}
            <iframe
              title="msg-body"
              srcdoc={selected.body_html}
              sandbox="allow-same-origin"
            ></iframe>
          {:else}
            <pre>{selected.body_text}</pre>
          {/if}
        </div>
      {:else}
        <div class="placeholder">Seleccioná un mensaje</div>
      {/if}
    </section>
  </div>
</div>

<style>
  .mail {
    height: 100%;
    width: 100%;
    display: flex;
    flex-direction: column;
    background: #1a1a1a;
    color: #e0e0e0;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 14px;
  }

  .mail-head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 12px;
    border-bottom: 1px solid #2a2a2a;
    background: #1f1f1f;
    height: 36px;
    flex-shrink: 0;
  }
  .mail-head button {
    background: #2a2a2a;
    color: #e0e0e0;
    border: none;
    padding: 4px 10px;
    border-radius: 4px;
    cursor: pointer;
  }
  .mail-head button:hover { background: #353535; }
  .mail-head button:disabled { opacity: 0.5; cursor: wait; }
  .count { color: #888; font-size: 12px; }
  .err { color: #ff6b6b; font-size: 12px; margin-left: auto; }

  .split {
    flex: 1;
    display: grid;
    grid-template-columns: 360px 1fr;
    overflow: hidden;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border-right: 1px solid #2a2a2a;
  }
  .empty { padding: 24px; color: #666; text-align: center; }

  .row {
    padding: 10px 12px;
    border-bottom: 1px solid #232323;
    cursor: pointer;
    line-height: 1.3;
  }
  .row:hover { background: #232323; }
  .row.active { background: #2d3a4a; }
  .row.unseen { background: #1d2530; }
  .row.unseen.active { background: #2d3a4a; }

  .row-top {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-weight: 500;
  }
  .row.unseen .row-top { font-weight: 700; }
  .row .from { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row .date { color: #888; font-size: 11px; flex-shrink: 0; }
  .row .subject {
    color: #ccc;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-top: 2px;
  }

  .reader {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .reader-head {
    padding: 14px 18px;
    border-bottom: 1px solid #2a2a2a;
    flex-shrink: 0;
  }
  .reader-head h2 {
    margin: 0 0 8px 0;
    font-size: 17px;
    line-height: 1.3;
  }
  .meta {
    color: #aaa;
    font-size: 12px;
    line-height: 1.6;
  }
  .meta b { color: #ccc; font-weight: 600; }
  .meta .date { margin-top: 6px; }

  .body {
    flex: 1;
    overflow: auto;
    padding: 0;
  }
  .body pre {
    margin: 0;
    padding: 18px;
    white-space: pre-wrap;
    word-wrap: break-word;
    color: #e0e0e0;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 14px;
    line-height: 1.55;
  }
  .body iframe {
    width: 100%;
    height: 100%;
    border: 0;
    background: #fff;
  }

  .placeholder {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #666;
  }
</style>

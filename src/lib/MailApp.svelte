<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    mailApi,
    formatDate,
    type MailHeader,
    type MailMessage,
    type OutgoingMessage,
  } from "./mail";

  type Props = { serviceId: string };
  let { serviceId }: Props = $props();

  let headers = $state<MailHeader[]>([]);
  let selected = $state<MailMessage | null>(null);
  let selectedId = $state<string | null>(null);
  let loadingList = $state(false);
  let loadingMsg = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Compose
  let composing = $state(false);
  let composeTo = $state("");
  let composeCc = $state("");
  let composeSubject = $state("");
  let composeBody = $state("");
  let composeReplyTo = $state<string | null>(null); // message_id RFC822
  let sending = $state(false);

  // Search
  let searchQuery = $state("");
  let searchActive = $state(false); // true cuando los headers son resultado de search

  let lastServiceId = "";
  let unlistens: UnlistenFn[] = [];

  $effect(() => {
    if (serviceId !== lastServiceId) {
      lastServiceId = serviceId;
      reset();
      loadInbox();
    }
  });

  onMount(async () => {
    unlistens.push(
      await listen<{ service_id: string; header: MailHeader }>(
        "kolibri:mail:new",
        (e) => {
          if (e.payload.service_id !== serviceId) return;
          // Insertar al tope si no existe ya.
          const exists = headers.some((h) => h.id === e.payload.header.id);
          if (!exists) {
            headers = [e.payload.header, ...headers];
          }
        },
      ),
    );
  });

  function reset() {
    headers = [];
    selected = null;
    selectedId = null;
    error = null;
    composing = false;
  }

  async function loadInbox() {
    searchActive = false;
    searchQuery = "";
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

  async function runSearch(e?: Event) {
    e?.preventDefault();
    const q = searchQuery.trim();
    if (!q) {
      await loadInbox();
      return;
    }
    loadingList = true;
    error = null;
    try {
      headers = await mailApi.search(serviceId, q, 0, 100);
      searchActive = true;
      selected = null;
      selectedId = null;
    } catch (e: any) {
      error = String(e);
    } finally {
      loadingList = false;
    }
  }

  function clearSearch() {
    if (searchActive || searchQuery) {
      loadInbox();
    }
  }

  async function open(h: MailHeader) {
    selectedId = h.id;
    selected = null;
    loadingMsg = true;
    composing = false;
    try {
      selected = await mailApi.getMessage(serviceId, h.id);
      // Auto-mark-read si estaba sin leer.
      if (!h.seen) {
        try {
          await mailApi.markRead(serviceId, h.id, true);
          updateHeaderLocal(h.id, { seen: true });
        } catch (_) { /* ignore */ }
      }
    } catch (e: any) {
      error = String(e);
    } finally {
      loadingMsg = false;
    }
  }

  function updateHeaderLocal(id: string, patch: Partial<MailHeader>) {
    headers = headers.map((h) => (h.id === id ? { ...h, ...patch } : h));
  }

  function removeHeaderLocal(id: string) {
    headers = headers.filter((h) => h.id !== id);
    if (selectedId === id) {
      selected = null;
      selectedId = null;
    }
  }

  async function toggleRead() {
    if (!selected || !selectedId) return;
    const cur = headers.find((h) => h.id === selectedId);
    const wantRead = !(cur?.seen ?? true);
    busy = true;
    try {
      await mailApi.markRead(serviceId, selectedId, wantRead);
      updateHeaderLocal(selectedId, { seen: wantRead });
    } catch (e: any) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function archiveCurrent() {
    if (!selectedId) return;
    busy = true;
    try {
      await mailApi.archive(serviceId, selectedId);
      removeHeaderLocal(selectedId);
    } catch (e: any) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function deleteCurrent() {
    if (!selectedId) return;
    busy = true;
    try {
      await mailApi.delete(serviceId, selectedId);
      removeHeaderLocal(selectedId);
    } catch (e: any) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function startReply(replyAll: boolean) {
    if (!selected) return;
    composing = true;
    composeTo = selected.from_addr;
    composeCc = replyAll ? selected.cc.join(", ") : "";
    composeSubject = selected.subject.startsWith("Re: ")
      ? selected.subject
      : "Re: " + selected.subject;
    composeReplyTo = selected.message_id ?? null;
    // Quote del original.
    const quoted = selected.body_text
      .split("\n")
      .map((l) => "> " + l)
      .join("\n");
    composeBody = `\n\n----- Original -----\nDe: ${selected.from_name} <${selected.from_addr}>\nFecha: ${formatDate(selected.date_ts)}\nAsunto: ${selected.subject}\n\n${quoted}`;
  }

  function cancelCompose() {
    composing = false;
    composeTo = "";
    composeCc = "";
    composeSubject = "";
    composeBody = "";
    composeReplyTo = null;
  }

  async function sendCompose(e: Event) {
    e.preventDefault();
    if (!composeTo.trim() || !composeSubject.trim()) return;
    sending = true;
    try {
      const msg: OutgoingMessage = {
        to: composeTo.split(",").map((s) => s.trim()).filter(Boolean),
        cc: composeCc.split(",").map((s) => s.trim()).filter(Boolean),
        subject: composeSubject,
        body_text: composeBody,
        in_reply_to: composeReplyTo,
      };
      await mailApi.send(serviceId, msg);
      cancelCompose();
    } catch (e: any) {
      error = String(e);
    } finally {
      sending = false;
    }
  }

  onDestroy(() => {
    reset();
    for (const u of unlistens) u();
    unlistens = [];
  });

  let currentSeen = $derived(
    selectedId ? headers.find((h) => h.id === selectedId)?.seen ?? true : true,
  );
</script>

<div class="mail">
  <header class="mail-head">
    <button onclick={loadInbox} disabled={loadingList || busy} title="Recargar">↻</button>
    <form class="search" onsubmit={runSearch}>
      <input
        type="search"
        placeholder="Buscar (Gmail: from:foo has:attachment | Outlook: KQL)"
        bind:value={searchQuery}
        disabled={loadingList}
      />
      {#if searchActive}
        <button type="button" onclick={clearSearch} title="Limpiar búsqueda">×</button>
      {/if}
    </form>
    <span class="count">
      {#if searchActive}{headers.length} resultados{:else}{headers.length} mensajes{/if}
    </span>
    {#if error}<span class="err" title={error}>{error}</span>{/if}
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
          {#if h.snippet}<div class="snippet">{h.snippet}</div>{/if}
        </li>
      {/each}
    </ul>

    <section class="reader">
      {#if composing}
        <form class="compose" onsubmit={sendCompose}>
          <div class="compose-head">
            <span class="compose-title">{composeReplyTo ? "Responder" : "Nuevo"}</span>
            <button type="button" class="iconbtn" onclick={cancelCompose} disabled={sending}>×</button>
          </div>
          <input type="text" placeholder="Para (separado por coma)" bind:value={composeTo} disabled={sending} required />
          <input type="text" placeholder="Cc" bind:value={composeCc} disabled={sending} />
          <input type="text" placeholder="Asunto" bind:value={composeSubject} disabled={sending} required />
          <textarea bind:value={composeBody} disabled={sending} rows="14"></textarea>
          <div class="compose-actions">
            <button type="submit" class="primary" disabled={sending || !composeTo.trim() || !composeSubject.trim()}>
              {sending ? "Enviando…" : "Enviar"}
            </button>
            <button type="button" onclick={cancelCompose} disabled={sending}>Cancelar</button>
          </div>
        </form>
      {:else if loadingMsg}
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
          <div class="toolbar">
            <button class="iconbtn" onclick={() => startReply(false)} disabled={busy}>↩ Responder</button>
            {#if selected.cc.length || selected.to.length > 1}
              <button class="iconbtn" onclick={() => startReply(true)} disabled={busy}>↩↩ Resp. todos</button>
            {/if}
            <button class="iconbtn" onclick={toggleRead} disabled={busy}>
              {currentSeen ? "Marcar no leído" : "Marcar leído"}
            </button>
            <button class="iconbtn" onclick={archiveCurrent} disabled={busy}>📁 Archivar</button>
            <button class="iconbtn danger" onclick={deleteCurrent} disabled={busy}>🗑 Eliminar</button>
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

  .search {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    max-width: 480px;
  }
  .search input {
    flex: 1;
    background: #1d1d1d;
    border: 1px solid #2f2f2f;
    border-radius: 4px;
    padding: 5px 10px;
    color: #e0e0e0;
    font-size: 12px;
    outline: none;
    height: 26px;
  }
  .search input:focus { border-color: #4a8; }
  .search button {
    height: 26px;
    width: 26px;
    padding: 0;
    background: #2a2a2a;
    color: #e0e0e0;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 14px;
  }
  .search button:hover { background: #353535; }
  .err {
    color: #ff6b6b;
    font-size: 12px;
    margin-left: auto;
    max-width: 420px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

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
  .row .snippet {
    color: #888;
    font-size: 12px;
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
    padding: 14px 18px 10px 18px;
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

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 12px;
  }
  .iconbtn {
    background: #2a2a2a;
    color: #e0e0e0;
    border: 1px solid #353535;
    padding: 5px 10px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
  }
  .iconbtn:hover { background: #353535; }
  .iconbtn:disabled { opacity: 0.4; cursor: not-allowed; }
  .iconbtn.danger { color: #ff8888; }
  .iconbtn.danger:hover { background: #4a1f1f; }

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

  /* Compose */
  .compose {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 14px 18px;
    gap: 8px;
    overflow: hidden;
  }
  .compose-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
  }
  .compose-title {
    font-weight: 600;
    color: #e0e0e0;
  }
  .compose input,
  .compose textarea {
    background: #1d1d1d;
    border: 1px solid #2f2f2f;
    border-radius: 4px;
    padding: 8px 10px;
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    resize: vertical;
    outline: none;
  }
  .compose input:focus,
  .compose textarea:focus { border-color: #4a8; }
  .compose textarea {
    flex: 1;
    min-height: 200px;
    line-height: 1.5;
  }
  .compose-actions {
    display: flex;
    gap: 8px;
  }
  .compose-actions button {
    background: #2a2a2a;
    color: #e0e0e0;
    border: 1px solid #353535;
    padding: 7px 14px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
  }
  .compose-actions button:hover { background: #353535; }
  .compose-actions button.primary {
    background: #4a8;
    border-color: #4a8;
    color: #fff;
    font-weight: 600;
  }
  .compose-actions button.primary:hover { background: #5b9; }
  .compose-actions button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>

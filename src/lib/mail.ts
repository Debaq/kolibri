// Tipos compartidos con el backend Rust (engines/mod.rs).

import { invoke } from "@tauri-apps/api/core";

export type MailHeader = {
  id: string;
  from_name: string;
  from_addr: string;
  subject: string;
  date_ts: number;
  snippet: string;
  seen: boolean;
  flagged: boolean;
  has_attachments: boolean;
  thread_id: string | null;
};

export type MailMessage = {
  id: string;
  message_id: string | null;
  from_name: string;
  from_addr: string;
  to: string[];
  cc: string[];
  subject: string;
  date_ts: number;
  body_text: string;
  body_html: string | null;
  has_attachments: boolean;
  thread_id: string | null;
};

export type ServiceEngine = "webview" | "imap" | "graph";

export type Service = {
  id: string;
  name: string;
  url: string;
  icon: string | null;
  color: string | null;
  engine: ServiceEngine;
  imap?: {
    email: string;
    client_id: string;
    client_secret: string;
    authorized: boolean;
  } | null;
  graph?: {
    email: string;
    client_id: string;
    tenant: string;
    authorized: boolean;
  } | null;
};

export const mailApi = {
  listInbox: (serviceId: string, offset = 0, limit = 50) =>
    invoke<MailHeader[]>("mail_list_inbox", { serviceId, offset, limit }),
  getMessage: (serviceId: string, id: string) =>
    invoke<MailMessage>("mail_get_message", { serviceId, id }),
  search: (serviceId: string, query: string, offset = 0, limit = 50) =>
    invoke<MailHeader[]>("mail_search", { serviceId, query, offset, limit }),
  markRead: (serviceId: string, id: string, read: boolean) =>
    invoke<void>("mail_mark_read", { serviceId, id, read }),
  archive: (serviceId: string, id: string) =>
    invoke<void>("mail_archive", { serviceId, id }),
  delete: (serviceId: string, id: string) =>
    invoke<void>("mail_delete", { serviceId, id }),
};

export function formatDate(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) {
    return d.toLocaleTimeString("es-CL", { hour: "2-digit", minute: "2-digit" });
  }
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleDateString("es-CL", {
    day: "2-digit",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

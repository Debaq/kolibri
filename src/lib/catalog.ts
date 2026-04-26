export type ServiceTemplate = {
  key: string;
  name: string;
  url: string;
  color: string;
  initial: string;
  description: string;
};

export const CATALOG: ServiceTemplate[] = [
  {
    key: "whatsapp",
    name: "WhatsApp",
    url: "https://web.whatsapp.com",
    color: "#25D366",
    initial: "W",
    description: "Mensajería WhatsApp Web",
  },
  {
    key: "gmail",
    name: "Gmail",
    url: "https://mail.google.com",
    color: "#EA4335",
    initial: "G",
    description: "Correo Google",
  },
  {
    key: "outlook",
    name: "Outlook",
    url: "https://outlook.live.com/mail",
    color: "#0078D4",
    initial: "O",
    description: "Correo Microsoft",
  },
];

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
  {
    key: "telegram",
    name: "Telegram",
    url: "https://web.telegram.org",
    color: "#229ED9",
    initial: "T",
    description: "Mensajería Telegram",
  },
  {
    key: "discord",
    name: "Discord",
    url: "https://discord.com/app",
    color: "#5865F2",
    initial: "D",
    description: "Chat de comunidades",
  },
  {
    key: "slack",
    name: "Slack",
    url: "https://app.slack.com",
    color: "#4A154B",
    initial: "S",
    description: "Mensajería de equipos",
  },
  {
    key: "messenger",
    name: "Messenger",
    url: "https://www.messenger.com",
    color: "#0084FF",
    initial: "M",
    description: "Mensajería Facebook",
  },
  {
    key: "instagram",
    name: "Instagram",
    url: "https://www.instagram.com/direct/inbox",
    color: "#E4405F",
    initial: "I",
    description: "Mensajes directos IG",
  },
  {
    key: "x",
    name: "X / Twitter",
    url: "https://x.com",
    color: "#000000",
    initial: "X",
    description: "Red social X",
  },
  {
    key: "linkedin",
    name: "LinkedIn",
    url: "https://www.linkedin.com",
    color: "#0A66C2",
    initial: "L",
    description: "Red profesional",
  },
  {
    key: "calendar",
    name: "Google Calendar",
    url: "https://calendar.google.com",
    color: "#4285F4",
    initial: "C",
    description: "Calendario Google",
  },
  {
    key: "notion",
    name: "Notion",
    url: "https://www.notion.so",
    color: "#000000",
    initial: "N",
    description: "Notas y wiki",
  },
  {
    key: "chatgpt",
    name: "ChatGPT",
    url: "https://chat.openai.com",
    color: "#10A37F",
    initial: "C",
    description: "Chat OpenAI",
  },
  {
    key: "claude",
    name: "Claude",
    url: "https://claude.ai",
    color: "#D97757",
    initial: "C",
    description: "Chat Anthropic",
  },
];

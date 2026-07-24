export type RegionId = "cn" | "intl";

export type ApiProtocol = "anthropic" | "openai";

export type ProviderGroup = "global" | "regional";

export interface AiProvider {
  id: string;
  name: { zh: string; en: string };
  regions: RegionId[];
  /** Chat / messages endpoint per region */
  endpoints: Partial<Record<RegionId, string>>;
  /** Models list endpoint per region (optional; derived from chat URL if missing) */
  modelsUrls?: Partial<Record<RegionId, string>>;
  defaultModel: string;
  /** Wire protocol for auth + request body */
  protocol: ApiProtocol;
  group: ProviderGroup;
}

export const AI_PROVIDERS: AiProvider[] = [
  {
    id: "claude",
    name: { zh: "Claude", en: "Claude" },
    regions: ["intl"],
    endpoints: {
      intl: "https://api.anthropic.com/v1/messages",
    },
    modelsUrls: {
      intl: "https://api.anthropic.com/v1/models",
    },
    defaultModel: "claude-sonnet-4-20250514",
    protocol: "anthropic",
    group: "global",
  },
  {
    id: "chatgpt",
    name: { zh: "ChatGPT", en: "ChatGPT" },
    regions: ["intl"],
    endpoints: {
      intl: "https://api.openai.com/v1/chat/completions",
    },
    modelsUrls: {
      intl: "https://api.openai.com/v1/models",
    },
    defaultModel: "gpt-4.1-mini",
    protocol: "openai",
    group: "global",
  },
  {
    id: "gemini",
    name: { zh: "Gemini", en: "Gemini" },
    regions: ["intl"],
    endpoints: {
      intl: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    },
    modelsUrls: {
      intl: "https://generativelanguage.googleapis.com/v1beta/openai/models",
    },
    defaultModel: "gemini-2.5-flash",
    protocol: "openai",
    group: "global",
  },
  {
    id: "opencode-go",
    name: { zh: "OpenCode Go", en: "OpenCode Go" },
    regions: ["intl"],
    endpoints: {
      intl: "https://opencode.ai/zen/go/v1/chat/completions",
    },
    modelsUrls: {
      intl: "https://opencode.ai/zen/go/v1/models",
    },
    defaultModel: "deepseek-v4-flash",
    protocol: "openai",
    group: "global",
  },
  {
    id: "openrouter",
    name: { zh: "OpenRouter", en: "OpenRouter" },
    regions: ["intl"],
    endpoints: {
      intl: "https://openrouter.ai/api/v1/chat/completions",
    },
    modelsUrls: {
      intl: "https://openrouter.ai/api/v1/models",
    },
    defaultModel: "anthropic/claude-sonnet-4",
    protocol: "openai",
    group: "global",
  },
  {
    id: "custom",
    name: { zh: "自定义", en: "Custom" },
    regions: ["intl"],
    endpoints: {},
    defaultModel: "",
    protocol: "anthropic",
    group: "global",
  },
  {
    id: "glm",
    name: { zh: "智谱 GLM", en: "Zhipu GLM" },
    regions: ["cn", "intl"],
    endpoints: {
      cn: "https://open.bigmodel.cn/api/anthropic/v1/messages",
      intl: "https://api.z.ai/api/anthropic/v1/messages",
    },
    modelsUrls: {
      cn: "https://open.bigmodel.cn/api/paas/v4/models",
      intl: "https://api.z.ai/api/paas/v4/models",
    },
    defaultModel: "glm-5",
    protocol: "anthropic",
    group: "regional",
  },
  {
    id: "minimax",
    name: { zh: "MiniMax", en: "MiniMax" },
    regions: ["cn", "intl"],
    endpoints: {
      cn: "https://api.minimaxi.com/anthropic/v1/messages",
      intl: "https://api.minimax.io/anthropic/v1/messages",
    },
    modelsUrls: {
      cn: "https://api.minimaxi.com/v1/models",
      intl: "https://api.minimax.io/v1/models",
    },
    defaultModel: "MiniMax-M2.7",
    protocol: "anthropic",
    group: "regional",
  },
  {
    id: "kimi",
    name: { zh: "Kimi", en: "Kimi" },
    regions: ["cn"],
    endpoints: {
      cn: "https://api.moonshot.cn/anthropic/v1/messages",
    },
    modelsUrls: {
      cn: "https://api.moonshot.cn/v1/models",
    },
    defaultModel: "kimi-k2.5",
    protocol: "anthropic",
    group: "regional",
  },
  {
    id: "deepseek",
    name: { zh: "DeepSeek", en: "DeepSeek" },
    regions: ["cn"],
    endpoints: {
      cn: "https://api.deepseek.com/anthropic/v1/messages",
    },
    modelsUrls: {
      cn: "https://api.deepseek.com/models",
    },
    defaultModel: "deepseek-v4-flash",
    protocol: "anthropic",
    group: "regional",
  },
];

export const API_PROTOCOLS: {
  id: ApiProtocol | "";
  label: { zh: string; en: string };
  desc: { zh: string; en: string };
}[] = [
  {
    id: "",
    label: { zh: "自动", en: "Auto" },
    desc: { zh: "根据 URL 自动检测", en: "Auto-detect from URL" },
  },
  {
    id: "anthropic",
    label: { zh: "Anthropic", en: "Anthropic" },
    desc: { zh: "x-api-key 认证", en: "x-api-key auth" },
  },
  {
    id: "openai",
    label: { zh: "OpenAI", en: "OpenAI" },
    desc: { zh: "Bearer Token 认证", en: "Bearer token auth" },
  },
];

export const REGION_LABELS: Record<RegionId, { zh: string; en: string }> = {
  cn: { zh: "国内", en: "China" },
  intl: { zh: "国际", en: "International" },
};

/** UI 그룹 순서: Custom은 Global 맨 뒤(배열 순서)에 포함 */
export const PROVIDER_GROUPS: ProviderGroup[] = ["global", "regional"];

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
  /** Whether the models catalog itself requires the provider API key (default: true). */
  modelsRequireApiKey?: boolean;
  /**
   * models.dev 공개 카탈로그 프로바이더 ID(지역별).
   * API 키가 없거나 공식 /models 조회가 실패했을 때 대표 모델 목록 fallback으로 사용한다.
   */
  catalogIds?: Partial<Record<RegionId, string>>;
  /** API 키가 없거나 실시간 조회가 실패해도 보여 줄 대표 모델 목록 */
  models?: string[];
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
    catalogIds: {
      intl: "anthropic",
    },
    models: [
      "claude-sonnet-4-20250514",
      "claude-opus-4-1-20250805",
      "claude-3-5-haiku-20241022",
    ],
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
    catalogIds: {
      intl: "openai",
    },
    models: ["gpt-5.1", "gpt-5-mini", "gpt-4.1", "gpt-4.1-mini"],
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
    catalogIds: {
      intl: "google",
    },
    models: [
      "gemini-3.6-flash",
      "gemini-3.5-flash",
      "gemini-3.5-flash-lite",
      "gemini-2.5-flash",
    ],
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
    modelsRequireApiKey: false,
    catalogIds: {
      intl: "opencode-go",
    },
    models: [
      "deepseek-v4-flash",
      "deepseek-v4-pro",
      "glm-5.2",
      "kimi-k3",
      "grok-4.5",
    ],
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
    catalogIds: {
      intl: "openrouter",
    },
    models: ["anthropic/claude-sonnet-4", "openrouter/auto", "openrouter/free"],
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
    catalogIds: {
      cn: "zhipuai",
      intl: "zai",
    },
    models: ["glm-5.1", "glm-5", "glm-5-turbo", "glm-4.7", "glm-4.7-flash"],
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
    catalogIds: {
      cn: "minimax-cn",
      intl: "minimax",
    },
    models: [
      "MiniMax-M2.7",
      "MiniMax-M2.7-highspeed",
      "MiniMax-M2.5",
      "MiniMax-M2.5-highspeed",
      "MiniMax-M2.1",
    ],
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
    catalogIds: {
      cn: "moonshotai-cn",
    },
    models: ["kimi-k2.5"],
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
    catalogIds: {
      cn: "deepseek",
    },
    models: ["deepseek-v4-pro", "deepseek-v4-flash"],
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

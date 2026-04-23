import { createTempArtifactTracker, finalizeLargeTextResult } from "./tool_result_storage.mjs";
import { detectSearchInterstitial, fetchSearchResponse } from "./web_search_transport.mjs";

const DEFAULT_LIMIT = 5;
const DEFAULT_MAX_RESULT_SIZE_CHARS = 12_000;

export const SEARCH_ENGINES = Object.freeze([
  { key: "baidu", name: "Baidu", region: "cn", parser: "baidu", url: "https://www.baidu.com/s?wd={keyword}" },
  { key: "bing_cn", name: "Bing CN", region: "cn", parser: "bing", url: "https://cn.bing.com/search?q={keyword}&ensearch=0" },
  { key: "bing_int", name: "Bing INT", region: "global", parser: "bing", url: "https://cn.bing.com/search?q={keyword}&ensearch=1" },
  { key: "so360", name: "360", region: "cn", parser: "generic", url: "https://www.so.com/s?q={keyword}" },
  { key: "sogou", name: "Sogou", region: "cn", parser: "sogou", url: "https://sogou.com/web?query={keyword}" },
  { key: "wechat", name: "WeChat", region: "cn", parser: "generic", url: "https://wx.sogou.com/weixin?type=2&query={keyword}" },
  { key: "toutiao", name: "Toutiao", region: "cn", parser: "generic", url: "https://so.toutiao.com/search?keyword={keyword}" },
  { key: "jisilu", name: "Jisilu", region: "cn", parser: "generic", url: "https://www.jisilu.cn/explore/?keyword={keyword}" },
  { key: "google", name: "Google", region: "global", parser: "google", url: "https://www.google.com/search?q={keyword}" },
  { key: "google_hk", name: "Google HK", region: "global", parser: "google", url: "https://www.google.com.hk/search?q={keyword}" },
  { key: "duckduckgo", name: "DuckDuckGo", region: "global", parser: "duckduckgo", url: "https://duckduckgo.com/html/?q={keyword}" },
  { key: "yahoo", name: "Yahoo", region: "global", parser: "generic", url: "https://search.yahoo.com/search?p={keyword}" },
  { key: "startpage", name: "Startpage", region: "global", parser: "generic", url: "https://www.startpage.com/sp/search?query={keyword}" },
  { key: "brave", name: "Brave", region: "global", parser: "generic", url: "https://search.brave.com/search?q={keyword}" },
  { key: "ecosia", name: "Ecosia", region: "global", parser: "generic", url: "https://www.ecosia.org/search?q={keyword}" },
  { key: "qwant", name: "Qwant", region: "global", parser: "generic", url: "https://www.qwant.com/?q={keyword}" },
  { key: "wolframalpha", name: "WolframAlpha", region: "global", parser: "generic", url: "https://www.wolframalpha.com/input?i={keyword}" },
]);

const SEARCH_ENGINE_BY_KEY = new Map(SEARCH_ENGINES.map((engine) => [engine.key, engine]));
const SEARCH_ENGINE_BY_NAME = new Map(SEARCH_ENGINES.map((engine) => [engine.name.toLowerCase(), engine]));
const DEFAULT_ENGINES_BY_REGION = {
  cn: ["baidu", "bing_cn", "sogou"],
  global: ["bing_int", "duckduckgo", "google"],
  all: ["bing_int", "duckduckgo", "google", "baidu"],
};

const GOOGLE_TIME_RANGE = {
  past_hour: "qdr:h",
  past_day: "qdr:d",
  past_week: "qdr:w",
  past_month: "qdr:m",
  past_year: "qdr:y",
};

const BRAVE_TIME_RANGE = {
  past_hour: "ph",
  past_day: "pd",
  past_week: "pw",
  past_month: "pm",
  past_year: "py",
};

const GOOGLE_SEARCH_TYPE = {
  images: "isch",
  news: "nws",
  videos: "vid",
};

const DUCKDUCKGO_SEARCH_TYPE = {
  web: "web",
  images: "images",
  news: "news",
  videos: "videos",
};

function stringEnumSchema(Type, values, description) {
  return Type.Union(
    values.map((value) => Type.Literal(value)),
    description ? { description } : undefined,
  );
}

export function createWebSearchParameters(Type) {
  const engineEnum = stringEnumSchema(
    Type,
    SEARCH_ENGINES.map((engine) => engine.key),
    "Search engine keys. Use multiple engines when comparing results.",
  );
  return Type.Object({
    query: Type.String({
      minLength: 1,
      description:
        "The main search query. Write it like a normal search request, for example 'site:github.com rust sqlx'.",
    }),
    engine: Type.Optional(engineEnum),
    engines: Type.Optional(
      Type.Array(engineEnum, {
        minItems: 1,
        maxItems: 6,
        description: "Optional list of engines to query in parallel.",
      }),
    ),
    region: Type.Optional(
      stringEnumSchema(
        Type,
        ["cn", "global", "all"],
        "Engine pool to use when engine/engines are not specified.",
      ),
    ),
    limit: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 10,
        description: "Maximum parsed results per engine. Default is 5.",
      }),
    ),
    site: Type.Optional(
      Type.String({
        description: "Optional site filter, for example 'github.com' or 'docs.rs'.",
      }),
    ),
    fileType: Type.Optional(
      Type.String({
        description: "Optional file type filter, for example 'pdf' or 'pptx'.",
      }),
    ),
    exactTerms: Type.Optional(
      Type.Array(Type.String(), {
        minItems: 1,
        maxItems: 8,
        description: "Terms wrapped in quotes for exact matching.",
      }),
    ),
    excludeTerms: Type.Optional(
      Type.Array(Type.String(), {
        minItems: 1,
        maxItems: 8,
        description: "Terms prefixed with '-' to exclude them from the search.",
      }),
    ),
    orTerms: Type.Optional(
      Type.Array(Type.String(), {
        minItems: 2,
        maxItems: 8,
        description: "Terms combined as '(a OR b OR c)'.",
      }),
    ),
    timeRange: Type.Optional(
      stringEnumSchema(
        Type,
        ["past_hour", "past_day", "past_week", "past_month", "past_year"],
        "Optional recency filter for engines that support it.",
      ),
    ),
    searchType: Type.Optional(
      stringEnumSchema(
        Type,
        ["web", "news", "images", "videos"],
        "Optional vertical search mode for engines that support it.",
      ),
    ),
    language: Type.Optional(
      Type.String({
        description: "Optional language or region hint such as 'zh-CN' or 'en-US'.",
      }),
    ),
    maxResultSizeChars: Type.Optional(
      Type.Integer({
        minimum: 1000,
        maximum: 200000,
        description:
          "Maximum characters returned directly to the model. Larger outputs are written to a temp file.",
      }),
    ),
  });
}

export function normalizeWebSearchInput(input) {
  const engineKeys = [];
  if (typeof input?.engine === "string" && input.engine.trim()) {
    engineKeys.push(input.engine.trim());
  }
  if (Array.isArray(input?.engines)) {
    for (const item of input.engines) {
      if (typeof item === "string" && item.trim()) {
        engineKeys.push(item.trim());
      }
    }
  }
  return {
    query: String(input?.query ?? "").trim(),
    engineKeys,
    region: normalizeRegion(input?.region),
    limit: clampNumber(input?.limit, 1, 10, DEFAULT_LIMIT),
    site: normalizeOptionalString(input?.site),
    fileType: normalizeOptionalString(input?.fileType),
    exactTerms: normalizeStringList(input?.exactTerms, 8),
    excludeTerms: normalizeStringList(input?.excludeTerms, 8),
    orTerms: normalizeStringList(input?.orTerms, 8),
    timeRange: normalizeEnum(
      input?.timeRange,
      ["past_hour", "past_day", "past_week", "past_month", "past_year"],
    ),
    searchType: normalizeEnum(input?.searchType, ["web", "news", "images", "videos"]) ?? "web",
    language: normalizeOptionalString(input?.language),
    maxResultSizeChars: clampNumber(
      input?.maxResultSizeChars,
      1000,
      200000,
      DEFAULT_MAX_RESULT_SIZE_CHARS,
    ),
  };
}

function normalizeRegion(region) {
  return normalizeEnum(region, ["cn", "global", "all"]) ?? "global";
}

function normalizeEnum(value, allowedValues) {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim().toLowerCase();
  return allowedValues.includes(trimmed) ? trimmed : undefined;
}

function normalizeOptionalString(value) {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function normalizeStringList(value, maxItems) {
  if (!Array.isArray(value)) {
    return [];
  }
  const items = [];
  for (const entry of value) {
    if (typeof entry !== "string") {
      continue;
    }
    const trimmed = entry.trim();
    if (!trimmed || items.includes(trimmed)) {
      continue;
    }
    items.push(trimmed);
    if (items.length >= maxItems) {
      break;
    }
  }
  return items;
}

function clampNumber(value, min, max, fallback) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.max(min, Math.min(max, Math.trunc(value)));
}

export function resolveEngines(normalizedInput) {
  const requested = normalizedInput.engineKeys
    .map(resolveEngineDefinition)
    .filter(Boolean);
  if (requested.length > 0) {
    return uniqueEngines(requested);
  }
  return DEFAULT_ENGINES_BY_REGION[normalizedInput.region]
    .map((key) => SEARCH_ENGINE_BY_KEY.get(key))
    .filter(Boolean);
}

function resolveEngineDefinition(value) {
  const raw = String(value ?? "").trim();
  if (!raw) {
    return undefined;
  }
  const normalized = raw.toLowerCase();
  return SEARCH_ENGINE_BY_KEY.get(normalized) ?? SEARCH_ENGINE_BY_NAME.get(normalized);
}

function uniqueEngines(engines) {
  const seen = new Set();
  return engines.filter((engine) => {
    if (seen.has(engine.key)) {
      return false;
    }
    seen.add(engine.key);
    return true;
  });
}

export function composeSearchQuery(normalizedInput) {
  const parts = [normalizedInput.query];
  if (normalizedInput.site) {
    parts.push(`site:${normalizedInput.site}`);
  }
  if (normalizedInput.fileType) {
    parts.push(`filetype:${normalizedInput.fileType}`);
  }
  for (const term of normalizedInput.exactTerms) {
    parts.push(`"${term}"`);
  }
  for (const term of normalizedInput.excludeTerms) {
    parts.push(`-${term}`);
  }
  if (normalizedInput.orTerms.length >= 2) {
    parts.push(`(${normalizedInput.orTerms.join(" OR ")})`);
  }
  return parts.filter(Boolean).join(" ").trim();
}

export function buildEngineUrl(engine, normalizedInput) {
  const query = composeSearchQuery(normalizedInput);
  const url = new URL(engine.url.replace("{keyword}", encodeURIComponent(query)));

  if (normalizedInput.language) {
    applyLanguageHint(engine, url, normalizedInput.language);
  }
  if (normalizedInput.timeRange) {
    applyTimeRange(engine, url, normalizedInput.timeRange);
  }
  if (normalizedInput.searchType && normalizedInput.searchType !== "web") {
    applySearchType(engine, url, normalizedInput.searchType);
  }

  return url.toString();
}

function applyLanguageHint(engine, url, language) {
  if (engine.key === "google" || engine.key === "google_hk") {
    url.searchParams.set("hl", language);
    return;
  }
  if (engine.key === "duckduckgo") {
    url.searchParams.set("kl", language.toLowerCase().replace("_", "-"));
    return;
  }
  if (engine.key === "bing_cn" || engine.key === "bing_int") {
    url.searchParams.set("setlang", language);
  }
}

function applyTimeRange(engine, url, timeRange) {
  if ((engine.key === "google" || engine.key === "google_hk") && GOOGLE_TIME_RANGE[timeRange]) {
    url.searchParams.set("tbs", GOOGLE_TIME_RANGE[timeRange]);
    return;
  }
  if (engine.key === "brave" && BRAVE_TIME_RANGE[timeRange]) {
    url.searchParams.set("tf", BRAVE_TIME_RANGE[timeRange]);
  }
}

function applySearchType(engine, url, searchType) {
  if ((engine.key === "google" || engine.key === "google_hk") && GOOGLE_SEARCH_TYPE[searchType]) {
    url.searchParams.set("tbm", GOOGLE_SEARCH_TYPE[searchType]);
    return;
  }
  if (engine.key === "duckduckgo" && DUCKDUCKGO_SEARCH_TYPE[searchType]) {
    url.searchParams.set("ia", DUCKDUCKGO_SEARCH_TYPE[searchType]);
    return;
  }
  if (engine.key === "brave") {
    url.searchParams.set("source", searchType);
  }
}

export function extractSearchResults(engine, html, limit) {
  const parsers = {
    baidu: extractBaiduResults,
    bing: extractBingResults,
    duckduckgo: extractDuckDuckGoResults,
    google: extractGoogleResults,
    sogou: extractSogouResults,
    generic: extractGenericResults,
  };
  const parser = parsers[engine.parser] ?? extractGenericResults;
  const parsed = uniqueResults(parser(html, limit));
  return parsed.slice(0, limit);
}

function extractGoogleResults(html, limit) {
  const results = [];
  const regex = /<a\b([^>]*?)href="([^"]+)"([^>]*)>([\s\S]*?)<\/a>/gi;
  let match;
  while ((match = regex.exec(html)) && results.length < limit) {
    const href = decodeGoogleRedirect(match[2]);
    if (!href || !href.startsWith("http")) {
      continue;
    }
    if (!/<h3[\s>]/i.test(match[4])) {
      continue;
    }
    const title = stripHtml(match[4]).trim();
    if (!title) {
      continue;
    }
    const nearby = html.slice(match.index, match.index + 1200);
    const snippet = extractSnippet(nearby, [
      /<div[^>]*class="[^"]*VwiC3b[^"]*"[^>]*>([\s\S]*?)<\/div>/i,
      /<span[^>]*class="[^"]*aCOpRe[^"]*"[^>]*>([\s\S]*?)<\/span>/i,
    ]);
    results.push({ title, url: href, snippet });
  }
  return results;
}

function extractDuckDuckGoResults(html, limit) {
  const results = [];
  const regex = /<a\b([^>]*class="[^"]*result__a[^"]*"[^>]*)href="([^"]+)"([^>]*)>([\s\S]*?)<\/a>/gi;
  let match;
  while ((match = regex.exec(html)) && results.length < limit) {
    const href = decodeDuckDuckGoRedirect(match[2]);
    if (!href) {
      continue;
    }
    const title = stripHtml(match[4]).trim();
    if (!title) {
      continue;
    }
    const nearby = html.slice(match.index, match.index + 2400);
    const snippet = extractSnippet(nearby, [
      /<a[^>]*class="[^"]*result__snippet[^"]*"[^>]*>([\s\S]*?)<\/a>/i,
      /<div[^>]*class="[^"]*result__snippet[^"]*"[^>]*>([\s\S]*?)<\/div>/i,
    ]);
    results.push({ title, url: href, snippet });
  }
  return results;
}

function extractSogouResults(html, limit) {
  const leftSection =
    html.match(/<!-- ResultListViewBegin -->([\s\S]*?)<!-- HintViewBegin -->/i)?.[1] ??
    html.match(/<!-- ResultListViewBegin -->([\s\S]*?)<!-- LeftResultViewEnd -->/i)?.[1] ??
    html;
  const blocks = leftSection.match(
    /<div class="vrwrap"[\s\S]*?(?:<!--STATUS VR OK-->|\s*(?=<div class="vrwrap"|<!-- HintViewBegin -->|<!-- LeftResultViewEnd -->))/gi,
  ) ?? [];
  const results = [];

  for (const block of blocks) {
    if (results.length >= limit) {
      break;
    }
    const titleMatch = block.match(/<h3[^>]*class="[^"]*vr-title[^"]*"[^>]*>[\s\S]*?<a[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/i);
    if (!titleMatch) {
      continue;
    }
    const title = stripHtml(titleMatch[2]).trim();
    if (!title || /看看(?:元宝|ima)怎么说/i.test(title)) {
      continue;
    }

    const directUrl =
      decodeHtmlEntities(block.match(/\bdata-url="([^"]+)"/i)?.[1] ?? "").trim() ||
      decodePossibleUrl(titleMatch[1]) ||
      "";
    const url = directUrl.startsWith("/")
      ? new URL(directUrl, "https://www.sogou.com").toString()
      : directUrl;
    if (!url) {
      continue;
    }

    const snippet = extractSnippet(block, [
      /<div[^>]*id="cacheresult_summary_[^"]*"[^>]*>([\s\S]*?)<\/div>/i,
      /<p[^>]*class="[^"]*star-wiki[^"]*"[^>]*>([\s\S]*?)<\/p>/i,
      /<div[^>]*class="[^"]*text-layout[^"]*"[^>]*>([\s\S]*?)<\/div>/i,
    ]);
    results.push({ title, url, snippet });
  }

  return results;
}

function extractBingResults(html, limit) {
  const results = [];
  const blocks = html.match(/<li[^>]*class="[^"]*b_algo[^"]*"[\s\S]*?<\/li>/gi) ?? [];
  for (const block of blocks) {
    if (results.length >= limit) {
      break;
    }
    const titleMatch = block.match(/<h2[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/i);
    if (!titleMatch) {
      continue;
    }
    const title = stripHtml(titleMatch[2]).trim();
    const url = decodePossibleUrl(titleMatch[1]);
    if (!title || !url) {
      continue;
    }
    const snippet = extractSnippet(block, [
      /<p>([\s\S]*?)<\/p>/i,
      /<div[^>]*class="[^"]*b_caption[^"]*"[^>]*>[\s\S]*?<p>([\s\S]*?)<\/p>/i,
    ]);
    results.push({ title, url, snippet });
  }
  return results;
}

function extractBaiduResults(html, limit) {
  const results = [];
  const blocks = html.match(/<div[^>]*class="[^"]*(?:result|c-container)[^"]*"[\s\S]*?<\/div>\s*<\/div>?/gi) ?? [];
  for (const block of blocks) {
    if (results.length >= limit) {
      break;
    }
    const titleMatch = block.match(/<h3[\s\S]*?<a[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/i);
    if (!titleMatch) {
      continue;
    }
    const title = stripHtml(titleMatch[2]).trim();
    const url = decodePossibleUrl(titleMatch[1]);
    if (!title || !url) {
      continue;
    }
    const snippet = extractSnippet(block, [
      /<div[^>]*class="[^"]*c-abstract[^"]*"[^>]*>([\s\S]*?)<\/div>/i,
      /<div[^>]*class="[^"]*content-right_[^"]*"[^>]*>([\s\S]*?)<\/div>/i,
    ]);
    results.push({ title, url, snippet });
  }
  return results;
}

function extractGenericResults(html, limit) {
  const results = [];
  const regex = /<a\b([^>]*?)href="([^"]+)"([^>]*)>([\s\S]*?)<\/a>/gi;
  let match;
  while ((match = regex.exec(html)) && results.length < limit * 3) {
    const href = decodePossibleUrl(match[2]);
    if (!href || !href.startsWith("http")) {
      continue;
    }
    const title = stripHtml(match[4]).trim();
    if (!title || title.length < 3) {
      continue;
    }
    if (isLikelySearchChrome(title, href)) {
      continue;
    }
    results.push({ title, url: href, snippet: "" });
  }
  return uniqueResults(results).slice(0, limit);
}

function uniqueResults(items) {
  const seen = new Set();
  return items.filter((item) => {
    const key = `${item.url}::${item.title}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

function isLikelySearchChrome(title, url) {
  const lowered = `${title} ${url}`.toLowerCase();
  return (
    lowered.includes("cached") ||
    lowered.includes("settings") ||
    lowered.includes("privacy") ||
    lowered.includes("feedback") ||
    lowered.includes("javascript") ||
    lowered.includes("search?q=")
  );
}

function extractSnippet(block, patterns) {
  for (const pattern of patterns) {
    const match = block.match(pattern);
    if (match?.[1]) {
      const text = stripHtml(match[1]).trim();
      if (text) {
        return text;
      }
    }
  }
  return "";
}

function stripHtml(value) {
  return decodeHtmlEntities(
    String(value ?? "")
      .replace(/<script[\s\S]*?<\/script>/gi, " ")
      .replace(/<style[\s\S]*?<\/style>/gi, " ")
      .replace(/<[^>]+>/g, " ")
      .replace(/\s+/g, " "),
  ).trim();
}

function decodeHtmlEntities(value) {
  return value
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/&quot;/gi, "\"")
    .replace(/&#39;/gi, "'")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&#(\d+);/g, (_match, code) => String.fromCharCode(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_match, code) => String.fromCharCode(parseInt(code, 16)));
}

function decodeGoogleRedirect(value) {
  if (!value) {
    return "";
  }
  try {
    if (value.startsWith("/url?")) {
      const url = new URL(`https://www.google.com${value}`);
      return decodePossibleUrl(url.searchParams.get("q") ?? "");
    }
  } catch {}
  return decodePossibleUrl(value);
}

function decodeDuckDuckGoRedirect(value) {
  if (!value) {
    return "";
  }
  try {
    if (value.includes("duckduckgo.com/l/?")) {
      const url = new URL(value, "https://duckduckgo.com");
      return decodePossibleUrl(url.searchParams.get("uddg") ?? "");
    }
  } catch {}
  return decodePossibleUrl(value);
}

function decodePossibleUrl(value) {
  if (!value) {
    return "";
  }
  const trimmed = decodeHtmlEntities(String(value).trim());
  try {
    return decodeURIComponent(trimmed);
  } catch {
    return trimmed;
  }
}

async function fetchEngineResults(engine, normalizedInput, deps, signal, tempArtifactTracker) {
  const response = await fetchSearchResponse(
    buildEngineUrl(engine, normalizedInput),
    normalizedInput.language,
    deps,
    signal,
    tempArtifactTracker,
  );
  const results = response.ok ? extractSearchResults(engine, response.html, normalizedInput.limit) : [];
  const interstitialError = response.ok ? detectSearchInterstitial(engine, response.html, results) : undefined;

  return {
    engine: {
      key: engine.key,
      name: engine.name,
      region: engine.region,
    },
    status: response.status,
    ok: response.ok,
    searchUrl: response.searchUrl,
    finalUrl: response.finalUrl,
    transport: response.transport,
    resultCount: results.length,
    results,
    error: response.ok ? interstitialError : `HTTP ${response.status}`,
  };
}

export function renderWebSearchText(normalizedInput, engineResponses) {
  const lines = [];
  lines.push(`Web search query: ${composeSearchQuery(normalizedInput)}`);
  lines.push(`Requested engines: ${engineResponses.map((item) => item.engine.name).join(", ")}`);

  for (const engineResponse of engineResponses) {
    lines.push("");
    lines.push(`[${engineResponse.engine.name}] ${engineResponse.searchUrl}`);
    if (engineResponse.error) {
      lines.push(`Error: ${engineResponse.error}`);
      continue;
    }
    if (engineResponse.results.length === 0) {
      lines.push("No parsed results. Search URL is provided above.");
      continue;
    }
    for (const [index, result] of engineResponse.results.entries()) {
      lines.push(`${index + 1}. ${result.title}`);
      lines.push(`   ${result.url}`);
      if (result.snippet) {
        lines.push(`   ${result.snippet}`);
      }
    }
  }

  return lines.join("\n").trim();
}

export function createWebSearchTool(deps) {
  const tempArtifactTracker = createTempArtifactTracker(deps);

  return {
    name: "web_search",
    label: "Web Search",
    description:
      "Search public web engines without API keys and return parsed result links plus snippets. " +
      "Use it when the task needs fresh external information, documentation, news, or source discovery. " +
      "Prefer this over bash/curl for broad web lookup. Do not use it for local files or when the user already provided the target URL.",
    promptSnippet:
      "Search public web engines like Google, Bing, DuckDuckGo, Baidu, Brave, Sogou, and WolframAlpha without API keys.",
    promptGuidelines: [
      "Use web_search when you need up-to-date public web information or to compare results across search engines.",
      "Add site, fileType, exactTerms, excludeTerms, or orTerms when you need more precise search control.",
      "Large outputs are written to a temp file in the workdir when possible; read that file instead of rerunning blindly.",
    ],
    parameters: createWebSearchParameters(deps.Type),
    async execute(_toolCallId, input, signal, _onUpdate, ctx) {
      const normalizedInput = normalizeWebSearchInput(input);
      if (!normalizedInput.query) {
        return {
          content: [{ type: "text", text: "Search query cannot be empty." }],
          details: { ok: false, reason: "empty_query" },
        };
      }

      const engines = resolveEngines(normalizedInput);
      const engineResponses = await Promise.all(
        engines.map(async (engine) => {
          try {
            return await fetchEngineResults(engine, normalizedInput, deps, signal, tempArtifactTracker);
          } catch (error) {
            return {
              engine: { key: engine.key, name: engine.name, region: engine.region },
              status: 0,
              ok: false,
              searchUrl: buildEngineUrl(engine, normalizedInput),
              finalUrl: buildEngineUrl(engine, normalizedInput),
              transport: typeof deps.execFileImpl === "function" ? "curl" : "fetch",
              resultCount: 0,
              results: [],
              error: error instanceof Error ? error.message : String(error),
            };
          }
        }),
      );

      const renderedText = renderWebSearchText(normalizedInput, engineResponses);
      const finalized = await finalizeLargeTextResult(
        renderedText,
        ctx,
        {
          filePrefix: "web-search",
          maxResultSizeChars: normalizedInput.maxResultSizeChars,
          tempArtifactTracker,
        },
        deps,
      );

      return {
        content: [{ type: "text", text: finalized.inlineText }],
        details: {
          ok: true,
          query: normalizedInput.query,
          renderedQuery: composeSearchQuery(normalizedInput),
          engines: engineResponses,
          storage: finalized.storage,
        },
      };
    },
  };
}

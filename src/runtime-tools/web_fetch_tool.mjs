import { fetchUrlWithCurl } from "./curl_http.mjs";
import { createTempArtifactTracker, finalizeLargeTextResult } from "./tool_result_storage.mjs";

const DEFAULT_MAX_RESULT_SIZE_CHARS = 12_000;
const DEFAULT_BROWSER_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 " +
  "(KHTML, like Gecko) Chrome/126.0 Safari/537.36";
const WECHAT_UA =
  "Mozilla/5.0 (Linux; Android 13; V2148A) AppleWebKit/537.36 Chrome/116.0.0.0 " +
  "Mobile Safari/537.36 MicroMessenger/8.0.49.2600 WeChat/arm64 Weixin NetType/WIFI Language/zh_CN";
const PROXY_ENV_KEYS = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"];
const BLOCK_TAGS = /<\/?(?:article|section|main|header|footer|nav|aside|div|p|li|ul|ol|h[1-6]|br|tr|table)[^>]*>/gi;

export function createWebFetchParameters(Type) {
  return Type.Object({
    url: Type.String({
      minLength: 1,
      description: "Absolute http(s) URL to fetch.",
    }),
    ua: Type.Optional(
      Type.String({
        minLength: 1,
        description:
          "Optional User-Agent override. Leave empty to use the default browser UA or the built-in WeChat UA for mp.weixin.qq.com articles.",
      }),
    ),
  });
}

export function normalizeWebFetchInput(input) {
  return {
    url: normalizeOptionalString(input?.url) ?? "",
    ua: normalizeOptionalString(input?.ua) ?? null,
  };
}

function normalizeOptionalString(value) {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function parseHttpUrl(value) {
  try {
    const url = new URL(value);
    if (url.protocol === "http:" || url.protocol === "https:") {
      return url;
    }
  } catch {}
  return null;
}

export function isWeChatArticleUrl(value) {
  const parsed = parseHttpUrl(value);
  return parsed?.hostname === "mp.weixin.qq.com";
}

export function resolveFetchUserAgent(normalizedInput) {
  if (normalizedInput.ua) {
    return normalizedInput.ua;
  }
  return isWeChatArticleUrl(normalizedInput.url) ? WECHAT_UA : DEFAULT_BROWSER_UA;
}

export function detectProxyEnvironment(env) {
  const activeKeys = PROXY_ENV_KEYS.filter((key) => normalizeOptionalString(env?.[key]));
  return {
    mode: activeKeys.length > 0 ? "environment_proxy" : "direct",
    envKeys: activeKeys,
  };
}

function decodeBody(bodyBuffer, contentType) {
  const contentTypeValue = String(contentType ?? "").toLowerCase();
  const charset = contentTypeValue.match(/charset=([^;]+)/)?.[1]?.trim();
  if (charset && charset !== "utf-8" && charset !== "utf8") {
    return {
      text: null,
      encoding: charset,
      binary: true,
    };
  }

  const text = bodyBuffer.toString("utf8");
  const binary = /\u0000/.test(text);
  return {
    text: binary ? null : text,
    encoding: "utf-8",
    binary,
  };
}

function decodeHtmlEntities(value) {
  return String(value ?? "")
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/&quot;/gi, "\"")
    .replace(/&#39;/gi, "'")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&#(\d+);/g, (_match, code) => String.fromCharCode(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_match, code) => String.fromCharCode(parseInt(code, 16)));
}

function stripHtml(value) {
  return decodeHtmlEntities(
    String(value ?? "")
      .replace(/<script[\s\S]*?<\/script>/gi, " ")
      .replace(/<style[\s\S]*?<\/style>/gi, " ")
      .replace(BLOCK_TAGS, "\n")
      .replace(/<[^>]+>/g, " ")
      .replace(/\r/g, "")
      .replace(/\n\s*\n+/g, "\n\n")
      .replace(/[ \t]+\n/g, "\n")
      .replace(/\s+/g, " "),
  ).trim();
}

function extractMetaTag(html, attrName, attrValue) {
  const pattern = new RegExp(
    `<meta[^>]*${attrName}=["']${escapeRegExp(attrValue)}["'][^>]*content=["']([^"']+)["'][^>]*>`,
    "i",
  );
  return decodeHtmlEntities(html.match(pattern)?.[1] ?? "").trim();
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function resolveUrl(value, baseUrl) {
  try {
    return new URL(value, baseUrl).toString();
  } catch {
    return "";
  }
}

function extractImages(html, baseUrl, limit = 50) {
  const images = [];
  const seen = new Set();
  const regex = /<(?:img|source)\b[^>]*(?:data-src|src)=["']([^"']+)["'][^>]*>/gi;
  let match;
  while ((match = regex.exec(html)) && images.length < limit) {
    const resolved = resolveUrl(decodeHtmlEntities(match[1]), baseUrl);
    if (!resolved || seen.has(resolved)) {
      continue;
    }
    seen.add(resolved);
    images.push(resolved);
  }
  return images;
}

function extractLinks(html, baseUrl, limit = 50) {
  const links = [];
  const seen = new Set();
  const regex = /<a\b[^>]*href=["']([^"']+)["'][^>]*>([\s\S]*?)<\/a>/gi;
  let match;
  while ((match = regex.exec(html)) && links.length < limit) {
    const href = resolveUrl(decodeHtmlEntities(match[1]), baseUrl);
    if (!href || seen.has(href)) {
      continue;
    }
    seen.add(href);
    links.push({
      url: href,
      text: stripHtml(match[2]).slice(0, 300),
    });
  }
  return links;
}

export function parseWeChatArticleHtml(html, baseUrl) {
  const title =
    decodeHtmlEntities(html.match(/msg_title = window\.title = ['"]([^'"]+)['"]/i)?.[1] ?? "").trim() ||
    extractMetaTag(html, "property", "og:title") ||
    extractMetaTag(html, "name", "twitter:title");
  const description = decodeHtmlEntities(extractMetaTag(html, "name", "description"))
    .replace(/\\x0a/g, "\n")
    .replace(/\\x26/g, "&")
    .trim();
  const author =
    decodeHtmlEntities(html.match(/nick_name: JsDecode\(['"]([^'"]+)['"]\)/i)?.[1] ?? "").trim() ||
    decodeHtmlEntities(html.match(/class="account_nickname_inner">([^<]+)</i)?.[1] ?? "").trim();
  const isVideo = /<h1[^>]*id="js_video_page_title"/i.test(html);
  const contentMatch = html.match(/id="js_content"[^>]*>([\s\S]*?)<\/div>\s*<\/div>\s*<\/div>/i);
  const contentText = isVideo
    ? description
    : stripHtml(contentMatch?.[1] ?? description);

  return {
    title,
    author,
    description,
    contentText,
    images: extractImages(html, baseUrl),
    links: extractLinks(html, baseUrl),
    isVideo,
  };
}

function parseHtmlDocument(html, baseUrl) {
  const articleHtml =
    html.match(/<article\b[^>]*>([\s\S]*?)<\/article>/i)?.[1] ??
    html.match(/<main\b[^>]*>([\s\S]*?)<\/main>/i)?.[1] ??
    html;
  const title =
    extractMetaTag(html, "property", "og:title") ||
    extractMetaTag(html, "name", "twitter:title") ||
    decodeHtmlEntities(html.match(/<title[^>]*>([\s\S]*?)<\/title>/i)?.[1] ?? "").trim();
  const author =
    extractMetaTag(html, "name", "author") ||
    extractMetaTag(html, "property", "article:author") ||
    extractMetaTag(html, "name", "twitter:creator");
  const description =
    extractMetaTag(html, "name", "description") ||
    extractMetaTag(html, "property", "og:description");
  const publishedTime =
    extractMetaTag(html, "property", "article:published_time") ||
    extractMetaTag(html, "name", "pubdate") ||
    null;

  return {
    title,
    author,
    description,
    publishedTime,
    contentText: stripHtml(articleHtml),
    images: extractImages(html, baseUrl),
    links: extractLinks(html, baseUrl),
  };
}

function safeParseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export function buildStructuredFetchResponse(normalizedInput, fetchResult, deps) {
  const proxy = detectProxyEnvironment(deps.processApi?.env ?? {});
  const effectiveUa = resolveFetchUserAgent(normalizedInput);
  const decoded = decodeBody(fetchResult.bodyBuffer, fetchResult.contentType);
  const bodyText = decoded.text;
  const loweredContentType = String(fetchResult.contentType ?? "").toLowerCase();
  const isWechat = isWeChatArticleUrl(fetchResult.finalUrl) || isWeChatArticleUrl(normalizedInput.url);

  let kind = "binary";
  let title = null;
  let author = null;
  let description = null;
  let publishedTime = null;
  let contentText = null;
  let parsedJson = null;
  let images = [];
  let links = [];
  let isVideo = null;
  let encoding = decoded.encoding;

  if (bodyText !== null) {
    const trimmed = bodyText.trim();
    if (isWechat) {
      kind = "wechat_article";
      const article = parseWeChatArticleHtml(bodyText, fetchResult.finalUrl);
      title = article.title || null;
      author = article.author || null;
      description = article.description || null;
      contentText = article.contentText || null;
      images = article.images;
      links = article.links;
      isVideo = article.isVideo;
    } else if (loweredContentType.includes("application/json") || /^[\[{]/.test(trimmed)) {
      kind = "json";
      parsedJson = safeParseJson(trimmed);
      contentText = parsedJson === null ? trimmed : JSON.stringify(parsedJson, null, 2);
    } else if (
      loweredContentType.includes("text/html") ||
      loweredContentType.includes("application/xhtml") ||
      /<html[\s>]|<body[\s>]|<article[\s>]/i.test(bodyText)
    ) {
      kind = "html";
      const document = parseHtmlDocument(bodyText, fetchResult.finalUrl);
      title = document.title || null;
      author = document.author || null;
      description = document.description || null;
      publishedTime = document.publishedTime;
      contentText = document.contentText || null;
      images = document.images;
      links = document.links;
    } else {
      kind = "text";
      contentText = trimmed;
    }
  }

  return {
    url: normalizedInput.url,
    ua: normalizedInput.ua,
    ok: fetchResult.status > 0,
    status: fetchResult.status,
    finalUrl: fetchResult.finalUrl,
    effectiveUa,
    proxy,
    kind,
    contentType: fetchResult.contentType,
    encoding,
    title,
    author,
    description,
    publishedTime,
    contentText,
    json: parsedJson,
    images,
    links,
    isVideo,
    rawBytes: fetchResult.bodyBuffer.length,
    rawTextLength: bodyText?.length ?? null,
    headers: fetchResult.headersText
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .slice(-20),
    error: null,
  };
}

export function createWebFetchTool(deps) {
  const tempArtifactTracker = createTempArtifactTracker(deps);

  return {
    name: "web_fetch",
    label: "Web Fetch",
    description:
      "Fetch a specific public URL and return structured content. " +
      "Use it when the user already gave the exact link and you need the page body, metadata, or parsed article text.",
    promptSnippet:
      "Fetch a specific URL with an optional User-Agent override and return a structured response.",
    promptGuidelines: [
      "Use web_fetch when the user already provided a full URL instead of searching first.",
      "Pass ua only when the target explicitly needs a custom User-Agent. If omitted, the tool uses a default browser UA, or a WeChat UA for mp.weixin.qq.com articles.",
      "The tool uses curl and inherits the current proxy environment, so overseas links can flow through the active VPN proxy settings.",
    ],
    parameters: createWebFetchParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const normalizedInput = normalizeWebFetchInput(input);
      const parsedUrl = parseHttpUrl(normalizedInput.url);
      if (!parsedUrl) {
        const invalid = {
          url: normalizedInput.url,
          ua: normalizedInput.ua,
          ok: false,
          status: 0,
          finalUrl: null,
          effectiveUa: resolveFetchUserAgent(normalizedInput),
          proxy: detectProxyEnvironment(deps.processApi?.env ?? {}),
          kind: "error",
          contentType: null,
          encoding: null,
          title: null,
          author: null,
          description: null,
          publishedTime: null,
          contentText: null,
          json: null,
          images: [],
          links: [],
          isVideo: null,
          rawBytes: 0,
          rawTextLength: null,
          headers: [],
          error: "url must be an absolute http(s) URL",
        };
        return {
          content: [{ type: "text", text: JSON.stringify(invalid, null, 2) }],
          details: { ok: false, reason: "invalid_url", url: normalizedInput.url },
        };
      }

      let responsePayload;
      try {
        const fetchResult = await fetchUrlWithCurl(
          parsedUrl.toString(),
          { userAgent: resolveFetchUserAgent(normalizedInput) },
          deps,
          tempArtifactTracker,
        );
        responsePayload = buildStructuredFetchResponse(normalizedInput, fetchResult, deps);
      } catch (error) {
        responsePayload = {
          url: normalizedInput.url,
          ua: normalizedInput.ua,
          ok: false,
          status: 0,
          finalUrl: null,
          effectiveUa: resolveFetchUserAgent(normalizedInput),
          proxy: detectProxyEnvironment(deps.processApi?.env ?? {}),
          kind: "error",
          contentType: null,
          encoding: null,
          title: null,
          author: null,
          description: null,
          publishedTime: null,
          contentText: null,
          json: null,
          images: [],
          links: [],
          isVideo: null,
          rawBytes: 0,
          rawTextLength: null,
          headers: [],
          error: error instanceof Error ? error.message : String(error),
        };
      }

      const rendered = JSON.stringify(responsePayload, null, 2);
      const finalized = await finalizeLargeTextResult(
        rendered,
        ctx,
        {
          filePrefix: "web-fetch",
          maxResultSizeChars: DEFAULT_MAX_RESULT_SIZE_CHARS,
          tempArtifactTracker,
        },
        deps,
      );

      return {
        content: [{ type: "text", text: finalized.inlineText }],
        details: {
          ok: responsePayload.ok,
          url: responsePayload.url,
          finalUrl: responsePayload.finalUrl,
          kind: responsePayload.kind,
          status: responsePayload.status,
          title: responsePayload.title,
          contentLength: responsePayload.contentText?.length ?? 0,
          storage: finalized.storage,
        },
      };
    },
  };
}

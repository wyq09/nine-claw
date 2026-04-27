import { fetchUrlWithCurl } from "./curl_http.mjs";

const WORKDIR_RESULT_DIR = ".nineclaw-tool-results";
const IMAGE_RESULT_DIR = "images";
const DEFAULT_IMAGE_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 " +
  "(KHTML, like Gecko) Chrome/126.0 Safari/537.36";

const CONTENT_TYPE_EXTENSION = {
  "image/jpeg": ".jpg",
  "image/jpg": ".jpg",
  "image/png": ".png",
  "image/webp": ".webp",
  "image/gif": ".gif",
  "image/svg+xml": ".svg",
  "image/avif": ".avif",
};

export function extractImageUrlsFromHtml(html, baseUrl, limit = 50) {
  const images = [];
  const seen = new Set();
  const regex = /<(?:img|source)\b[^>]*(?:data-src|data-original|srcset|src)=["']([^"']+)["'][^>]*>/gi;
  let match;
  while ((match = regex.exec(html)) && images.length < limit) {
    const resolved = resolveImageCandidate(decodeHtmlEntities(match[1]), baseUrl);
    if (!resolved || seen.has(resolved)) {
      continue;
    }
    seen.add(resolved);
    images.push(resolved);
  }
  return images;
}

export async function downloadImagesToWorkdir(imageUrls, ctx, options, deps, tempArtifactTracker) {
  const limit = clampNumber(options?.limit, 0, 20, 6);
  const uniqueUrls = uniqueHttpUrls(imageUrls).slice(0, limit);
  if (uniqueUrls.length === 0) {
    return [];
  }

  const targetDir = await resolveImageDirectory(ctx, deps);
  const downloads = [];
  for (const [index, url] of uniqueUrls.entries()) {
    try {
      const image = await fetchImage(url, options, deps, tempArtifactTracker);
      const extension = resolveImageExtension(url, image.contentType);
      const fileName = `${String(index + 1).padStart(2, "0")}-${hashUrl(url, deps)}${extension}`;
      const filePath = deps.pathApi.join(targetDir, fileName);
      await deps.withFileMutationQueue(filePath, async () => {
        await deps.fsPromises.writeFile(filePath, image.bodyBuffer);
      });
      downloads.push({
        url,
        ok: true,
        filePath,
        contentType: image.contentType,
        bytes: image.bodyBuffer.length,
        error: null,
      });
    } catch (error) {
      downloads.push({
        url,
        ok: false,
        filePath: null,
        contentType: null,
        bytes: 0,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
  return downloads;
}

function decodeHtmlEntities(value) {
  return String(value ?? "")
    .replace(/&amp;/gi, "&")
    .replace(/&quot;/gi, "\"")
    .replace(/&#39;/gi, "'")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&#(\d+);/g, (_match, code) => String.fromCharCode(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_match, code) => String.fromCharCode(parseInt(code, 16)));
}

function resolveImageCandidate(value, baseUrl) {
  const firstSrcsetCandidate = String(value ?? "").split(",")[0]?.trim().split(/\s+/)[0] ?? "";
  if (!firstSrcsetCandidate || firstSrcsetCandidate.startsWith("data:")) {
    return "";
  }
  try {
    const url = new URL(firstSrcsetCandidate, baseUrl);
    return url.protocol === "http:" || url.protocol === "https:" ? url.toString() : "";
  } catch {
    return "";
  }
}

function uniqueHttpUrls(imageUrls) {
  const seen = new Set();
  const unique = [];
  for (const imageUrl of imageUrls) {
    try {
      const parsed = new URL(String(imageUrl));
      if ((parsed.protocol !== "http:" && parsed.protocol !== "https:") || seen.has(parsed.toString())) {
        continue;
      }
      seen.add(parsed.toString());
      unique.push(parsed.toString());
    } catch {}
  }
  return unique;
}

async function resolveImageDirectory(ctx, deps) {
  const resultDirInWorkdir = deps.pathApi.resolve(ctx?.cwd || ".", WORKDIR_RESULT_DIR, IMAGE_RESULT_DIR);
  const fallbackDir = deps.pathApi.join(deps.osApi.tmpdir(), WORKDIR_RESULT_DIR, IMAGE_RESULT_DIR);
  const targetDir = (await isWritableDirectory(resultDirInWorkdir, deps)) ? resultDirInWorkdir : fallbackDir;
  await deps.fsPromises.mkdir(targetDir, { recursive: true });
  return targetDir;
}

async function isWritableDirectory(dirPath, deps) {
  try {
    await deps.fsPromises.mkdir(dirPath, { recursive: true });
    await deps.fsPromises.access(dirPath, deps.fsConstants.W_OK);
    return true;
  } catch {
    return false;
  }
}

async function fetchImage(url, options, deps, tempArtifactTracker) {
  if (typeof deps.execFileImpl === "function") {
    return fetchUrlWithCurl(
      url,
      { userAgent: options?.userAgent ?? DEFAULT_IMAGE_UA, headers: options?.headers },
      deps,
      tempArtifactTracker,
    );
  }

  if (typeof deps.fetchImpl !== "function") {
    throw new Error("No image download transport available");
  }
  const response = await deps.fetchImpl(url, { headers: { "user-agent": options?.userAgent ?? DEFAULT_IMAGE_UA } });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  const bodyBuffer = Buffer.from(await response.arrayBuffer());
  return {
    status: response.status,
    contentType: response.headers?.get?.("content-type") ?? null,
    finalUrl: response.url ?? url,
    headersText: "",
    bodyBuffer,
    transport: "fetch",
  };
}

function resolveImageExtension(url, contentType) {
  const normalizedType = String(contentType ?? "").split(";")[0].trim().toLowerCase();
  if (CONTENT_TYPE_EXTENSION[normalizedType]) {
    return CONTENT_TYPE_EXTENSION[normalizedType];
  }
  const pathname = new URL(url).pathname.toLowerCase();
  const extension = pathname.match(/\.(jpe?g|png|webp|gif|svg|avif)$/i)?.[0];
  return extension ?? ".img";
}

function hashUrl(url, deps) {
  if (deps.cryptoApi?.createHash) {
    return deps.cryptoApi.createHash("sha1").update(url).digest("hex").slice(0, 12);
  }
  return deps.cryptoApi.randomBytes(6).toString("hex");
}

function clampNumber(value, min, max, fallback) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.max(min, Math.min(max, Math.trunc(value)));
}

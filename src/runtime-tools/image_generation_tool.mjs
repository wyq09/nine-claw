const DEFAULT_OUTPUT_DIR = ".nineclaw-generated-images";
const DEFAULT_SIZE = "1:1";
const DEFAULT_COUNT = 1;

export function createImageGenerationParameters(Type) {
  return Type.Object({
    prompt: Type.String({
      minLength: 1,
      description: "Image prompt in natural language.",
    }),
    size: Type.Optional(
      Type.String({
        minLength: 3,
        description:
          "Optional aspect ratio, e.g. 1:1, 16:9, 9:16, 3:2, 2:3, 4:3, 3:4, 5:4, 4:5, 2:1, 1:2, 21:9, 9:21.",
      }),
    ),
    resolution: Type.Optional(
      Type.Union([
        Type.Literal("1k"),
        Type.Literal("2k"),
        Type.Literal("4k"),
      ]),
    ),
    background: Type.Optional(
      Type.Union([
        Type.Literal("auto"),
        Type.Literal("transparent"),
        Type.Literal("opaque"),
      ]),
    ),
    outputFormat: Type.Optional(
      Type.Union([
        Type.Literal("png"),
        Type.Literal("jpeg"),
        Type.Literal("webp"),
      ]),
    ),
    quality: Type.Optional(
      Type.Union([
        Type.Literal("auto"),
        Type.Literal("low"),
        Type.Literal("medium"),
        Type.Literal("high"),
      ]),
    ),
    moderation: Type.Optional(
      Type.Union([Type.Literal("auto"), Type.Literal("low")]),
    ),
    outputCompression: Type.Optional(
      Type.Integer({
        minimum: 0,
        maximum: 100,
      }),
    ),
    count: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 4,
      }),
    ),
    negativePrompt: Type.Optional(
      Type.String({
        minLength: 1,
        description: "Optional negative prompt.",
      }),
    ),
    seed: Type.Optional(
      Type.Integer({
        description: "Optional deterministic seed when the upstream gateway supports it.",
      }),
    ),
    imageUrls: Type.Optional(
      Type.Array(
        Type.String({
          minLength: 1,
          description: "Optional public reference image URLs for image-to-image workflows.",
        }),
        { maxItems: 16 },
      ),
    ),
    maskUrl: Type.Optional(
      Type.String({
        minLength: 1,
        description: "Optional public mask URL for inpainting workflows.",
      }),
    ),
  });
}

export function normalizeImageGenerationInput(input) {
  return {
    prompt: String(input?.prompt ?? "").trim(),
    size: normalizeOptionalString(input?.size),
    resolution: normalizeEnum(input?.resolution, ["1k", "2k", "4k"]),
    background: normalizeEnum(input?.background, ["auto", "transparent", "opaque"]),
    outputFormat: normalizeEnum(input?.outputFormat, ["png", "jpeg", "webp"]),
    quality: normalizeEnum(input?.quality, ["auto", "low", "medium", "high"]),
    moderation: normalizeEnum(input?.moderation, ["auto", "low"]),
    outputCompression: clampCompression(input?.outputCompression),
    count: clampCount(input?.count),
    negativePrompt: normalizeOptionalString(input?.negativePrompt),
    seed: Number.isInteger(input?.seed) ? input.seed : undefined,
    imageUrls: normalizeUrlArray(input?.imageUrls),
    maskUrl: normalizeOptionalString(input?.maskUrl),
  };
}

function normalizeOptionalString(value) {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function normalizeEnum(value, allowed) {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim().toLowerCase();
  return allowed.includes(trimmed) ? trimmed : undefined;
}

function clampCount(value) {
  if (!Number.isFinite(value)) {
    return undefined;
  }
  return Math.max(1, Math.min(4, Math.trunc(value)));
}

function clampCompression(value) {
  if (!Number.isFinite(value)) {
    return undefined;
  }
  return Math.max(0, Math.min(100, Math.trunc(value)));
}

function normalizeUrlArray(value) {
  if (!Array.isArray(value)) {
    return undefined;
  }
  const normalized = value
    .map((item) => normalizeOptionalString(item))
    .filter(Boolean)
    .slice(0, 16);
  return normalized.length > 0 ? normalized : undefined;
}

function proxyUrl(env) {
  const base = env?.NINECLAW_PROXY_BASE_URL?.trim();
  const token = env?.NINECLAW_PROXY_SESSION_TOKEN?.trim();
  if (!base || !token) {
    return null;
  }
  return `${base}/image/${token}/generate`;
}

function extensionFromMimeType(mimeType) {
  const lowered = String(mimeType ?? "").toLowerCase();
  if (lowered.includes("jpeg")) return "jpg";
  if (lowered.includes("webp")) return "webp";
  return "png";
}

function outputDirForContext(ctx, deps) {
  const cwd = typeof ctx?.cwd === "string" && ctx.cwd.trim() ? ctx.cwd : deps.processApi?.cwd?.();
  if (cwd) {
    return deps.pathApi.join(cwd, DEFAULT_OUTPUT_DIR);
  }
  return deps.pathApi.join(deps.osApi.tmpdir(), DEFAULT_OUTPUT_DIR);
}

async function saveGeneratedImage(image, index, ctx, deps) {
  const outputDir = outputDirForContext(ctx, deps);
  const stamp = deps.cryptoApi.randomBytes(4).toString("hex");
  const ext = extensionFromMimeType(image.mimeType);
  const filePath = deps.pathApi.join(outputDir, `image-${Date.now()}-${index + 1}-${stamp}.${ext}`);
  await deps.withFileMutationQueue(filePath, async () => {
    await deps.fsPromises.mkdir(outputDir, { recursive: true });
    await deps.fsPromises.writeFile(filePath, Buffer.from(image.dataBase64, "base64"));
  });
  return filePath;
}

export function createImageGenerationTool(deps) {
  return {
    name: "image_generate",
    label: "Image Generate",
    description: "Generate images through NineClaw's system-configured image gateway.",
    promptSnippet: "Generate an image with the system default image provider and model.",
    promptGuidelines: [
      "Use this tool when the user asks for image generation or visual mockups.",
      "Do not ask for provider IDs or API keys. The tool reads the system default image model.",
    ],
    parameters: createImageGenerationParameters(deps.Type),
    async execute(_toolCallId, params, signal, onUpdate, ctx) {
      const normalized = normalizeImageGenerationInput(params);
      if (!normalized.prompt) {
        return {
          content: [{ type: "text", text: "Image prompt cannot be empty." }],
          details: { ok: false, error: "prompt is required" },
        };
      }

      const url = proxyUrl(deps.processApi?.env ?? {});
      if (!url) {
        return {
          content: [{ type: "text", text: "Image gateway is not available in the current session." }],
          details: { ok: false, error: "missing image gateway env" },
        };
      }

      onUpdate?.({
        content: [{ type: "text", text: "Generating image with the system default provider..." }],
        details: {
          size: normalized.size ?? DEFAULT_SIZE,
          resolution: normalized.resolution ?? "1k",
          count: normalized.count ?? DEFAULT_COUNT,
        },
      });

      const response = await deps.fetchImpl(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(normalized),
        signal,
      });
      const text = await response.text();
      if (!response.ok) {
        throw new Error(text || `Image gateway request failed with status ${response.status}`);
      }

      const parsed = JSON.parse(text);
      const images = Array.isArray(parsed?.images) ? parsed.images : [];
      if (images.length === 0) {
        throw new Error("Image gateway returned no images.");
      }

      const savedPaths = [];
      for (let index = 0; index < images.length; index += 1) {
        const filePath = await saveGeneratedImage(images[index], index, ctx, deps);
        savedPaths.push(filePath);
      }

      const summary = [
        `Generated ${images.length} image${images.length > 1 ? "s" : ""}.`,
        `Provider: ${parsed.providerId || "system-default"}.`,
        `Model: ${parsed.model || "unknown"}.`,
        `Saved to: ${savedPaths.join(", ")}.`,
      ];
      if (parsed.taskId) {
        summary.push(`Task ID: ${parsed.taskId}.`);
      }
      if (parsed.revisedPrompt) {
        summary.push(`Revised prompt: ${parsed.revisedPrompt}`);
      }

      return {
        content: [
          { type: "text", text: summary.join(" ") },
          ...images.map((image) => ({
            type: "image",
            data: image.dataBase64,
            mimeType: image.mimeType || "image/png",
          })),
        ],
        details: {
          providerId: parsed.providerId,
          adapterType: parsed.adapterType,
          model: parsed.model,
          taskId: parsed.taskId,
          savedPaths,
        },
      };
    },
  };
}

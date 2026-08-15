const MAX_IMAGES = 6;

export function createImageVisionParameters(Type) {
  return Type.Object({
    images: Type.Array(
      Type.String({
        minLength: 1,
        description:
          "Image or video to analyze: an absolute local file path from the conversation, or an http(s) image URL.",
      }),
      { minItems: 1, maxItems: 6 },
    ),
    question: Type.Optional(
      Type.String({
        minLength: 1,
        description:
          "Optional question about the media, e.g. what text appears in the screenshot. Defaults to a full content description.",
      }),
    ),
  });
}

export function normalizeImageVisionInput(input) {
  const rawList = Array.isArray(input?.images) ? input.images : [];
  const images = rawList
    .map((item) => (typeof item === "string" ? item.trim() : ""))
    .filter(Boolean)
    .slice(0, MAX_IMAGES);
  const question =
    typeof input?.question === "string" && input.question.trim()
      ? input.question.trim()
      : undefined;
  return { images, question };
}

export function toVisionSources(images) {
  return images.map((value) =>
    /^https?:\/\//i.test(value) ? { url: value } : { path: value },
  );
}

function visionProxyUrl(env) {
  const base = env?.NINECLAW_PROXY_BASE_URL?.trim();
  const token = env?.NINECLAW_PROXY_SESSION_TOKEN?.trim();
  if (!base || !token) {
    return null;
  }
  return `${base}/vision/${token}/describe`;
}

export function createImageVisionTool(deps) {
  return {
    name: "image_analyze",
    label: "Image Analyze",
    description:
      "Analyze images or videos through NineClaw's system-configured vision model and return a text description. Use it when you cannot view media directly.",
    promptSnippet:
      "Recognize image/video content via the system vision model when the current model cannot see media.",
    promptGuidelines: [
      "When the user message contains image or video attachment paths (markdown links) or image URLs and you cannot view media directly, call image_analyze with those paths before answering.",
      "Pass file paths exactly as they appear in the conversation; the system vision model reads them for you.",
      "For screenshots or photos containing text, ask the vision model to transcribe the text verbatim via the question parameter.",
      "Do not ask the user to describe the image when image_analyze is available; if the tool reports the vision model is not configured, tell the user to configure it in settings.",
    ],
    parameters: createImageVisionParameters(deps.Type),
    async execute(_toolCallId, params, signal, onUpdate, _ctx) {
      const normalized = normalizeImageVisionInput(params);
      if (normalized.images.length === 0) {
        return {
          content: [{ type: "text", text: "No image paths or URLs were provided." }],
          details: { ok: false, error: "images is required" },
        };
      }

      const url = visionProxyUrl(deps.processApi?.env ?? {});
      if (!url) {
        return {
          content: [
            {
              type: "text",
              text: "Vision gateway is not available in the current session.",
            },
          ],
          details: { ok: false, error: "missing vision gateway env" },
        };
      }

      onUpdate?.({
        content: [
          {
            type: "text",
            text: `Analyzing ${normalized.images.length} image(s) with the system vision model...`,
          },
        ],
        details: { imageCount: normalized.images.length },
      });

      const response = await deps.fetchImpl(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          images: toVisionSources(normalized.images),
          prompt: normalized.question,
        }),
        signal,
      });
      const text = await response.text();
      if (!response.ok) {
        if (response.status === 400 && text.includes("no image vision runtime")) {
          return {
            content: [
              {
                type: "text",
                text: "The system vision model is not configured yet. Ask the user to configure it in Settings → Providers → 识图模型 (base URL, API key, vision model name), then retry.",
              },
            ],
            details: { ok: false, error: "vision model not configured" },
          };
        }
        throw new Error(text || `Vision gateway request failed with status ${response.status}`);
      }

      const parsed = JSON.parse(text);
      const description = typeof parsed?.text === "string" ? parsed.text.trim() : "";
      if (!description) {
        throw new Error("Vision gateway returned an empty description.");
      }

      const summary = [`[image_analyze] model=${parsed.model || "unknown"} images=${parsed.imageCount || normalized.images.length}`];
      if (Array.isArray(parsed.notes) && parsed.notes.length > 0) {
        summary.push(`Notes: ${parsed.notes.join("; ")}`);
      }
      summary.push("", description);

      return {
        content: [{ type: "text", text: summary.join("\n") }],
        details: {
          ok: true,
          model: parsed.model,
          apiFormat: parsed.apiFormat,
          imageCount: parsed.imageCount,
          notes: parsed.notes || [],
        },
      };
    },
  };
}
const DEFAULT_OUTPUT_DIR = ".nineclaw-generated-images";
const POLL_INTERVAL_MS = 4000;
const MAX_POLL_ATTEMPTS = 60;

export function createImageTaskQueryParameters(Type) {
  return Type.Object({
    taskId: Type.Union([
      Type.String({
        minLength: 1,
        description: "A single task ID to query.",
      }),
      Type.Array(
        Type.String({
          minLength: 1,
          description: "Task IDs to query in parallel.",
        }),
        { minItems: 1, maxItems: 10 },
      ),
    ], {
      description:
        "One or more task IDs from previous image generation requests. Pass a string for a single task or an array for batch async querying.",
    }),
  });
}

function taskQueryProxyUrl(env) {
  const base = env?.NINECLAW_PROXY_BASE_URL?.trim();
  const token = env?.NINECLAW_PROXY_SESSION_TOKEN?.trim();
  if (!base || !token) return null;
  return `${base}/image/${token}/task`;
}

function extensionFromMimeType(mimeType) {
  const lowered = String(mimeType ?? "").toLowerCase();
  if (lowered.includes("jpeg")) return "jpg";
  if (lowered.includes("webp")) return "webp";
  return "png";
}

function outputDirForContext(ctx, deps) {
  const cwd =
    typeof ctx?.cwd === "string" && ctx.cwd.trim() ? ctx.cwd : deps.processApi?.cwd?.();
  if (cwd) return deps.pathApi.join(cwd, DEFAULT_OUTPUT_DIR);
  return deps.pathApi.join(deps.osApi.tmpdir(), DEFAULT_OUTPUT_DIR);
}

async function saveGeneratedImage(image, index, ctx, deps) {
  const outputDir = outputDirForContext(ctx, deps);
  const stamp = deps.cryptoApi.randomBytes(4).toString("hex");
  const ext = extensionFromMimeType(image.mimeType);
  const filePath = deps.pathApi.join(
    outputDir,
    `image-${Date.now()}-${index + 1}-${stamp}.${ext}`,
  );
  await deps.withFileMutationQueue(filePath, async () => {
    await deps.fsPromises.mkdir(outputDir, { recursive: true });
    await deps.fsPromises.writeFile(filePath, Buffer.from(image.dataBase64, "base64"));
  });
  return filePath;
}

function normalizeTaskIds(raw) {
  if (Array.isArray(raw)) {
    return raw.map((id) => String(id ?? "").trim()).filter((id) => id.length > 0);
  }
  const single = String(raw ?? "").trim();
  return single.length > 0 ? [single] : [];
}

async function pollSingleTask(taskId, baseUrl, signal, deps, onUpdate, ctx) {
  const url = `${baseUrl}/${encodeURIComponent(taskId)}`;

  for (let attempt = 0; attempt < MAX_POLL_ATTEMPTS; attempt += 1) {
    const response = await deps.fetchImpl(url, {
      method: "GET",
      headers: { "content-type": "application/json" },
      signal,
    });
    const text = await response.text();
    if (!response.ok) {
      return { taskId, ok: false, status: "error", error: text || `HTTP ${response.status}` };
    }

    const parsed = JSON.parse(text);
    const { status, progress, error: taskError, images } = parsed;

    if (status === "completed") {
      if (!images || images.length === 0) {
        return { taskId, ok: false, status, error: "no images in result" };
      }
      const savedPaths = [];
      for (let index = 0; index < images.length; index += 1) {
        const filePath = await saveGeneratedImage(images[index], index, ctx, deps);
        savedPaths.push(filePath);
      }
      return { taskId, ok: true, status, images, savedPaths };
    }

    if (status === "failed" || status === "cancelled") {
      return { taskId, ok: false, status, error: taskError || "unknown error" };
    }

    onUpdate?.({
      content: [
        {
          type: "text",
          text: `任务 ${taskId}: ${status}${progress != null ? ` (${progress}%)` : ""}`,
        },
      ],
      details: { taskId, status, progress, attempt: attempt + 1 },
    });

    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }

  return { taskId, ok: false, status: "timeout", error: "polling timed out" };
}

export function createImageTaskQueryTool(deps) {
  return {
    name: "image_task_query",
    label: "Image Task Query",
    description:
      "Query one or more asynchronous image generation tasks by task_id. Supports batch async querying — all tasks are polled concurrently, and results are returned when all complete.",
    promptSnippet:
      "Check the status of previously submitted image generation task(s) by task_id.",
    promptGuidelines: [
      "Use this tool when the user asks about the status of image generation tasks.",
      "Pass a single taskId string for one task, or an array of taskIds for batch querying.",
      "All tasks are polled concurrently — results come back together.",
    ],
    parameters: createImageTaskQueryParameters(deps.Type),
    async execute(_toolCallId, params, signal, onUpdate, ctx) {
      const taskIds = normalizeTaskIds(params?.taskId);
      if (taskIds.length === 0) {
        return {
          content: [{ type: "text", text: "taskId is required." }],
          details: { ok: false, error: "taskId is required" },
        };
      }

      const baseUrl = taskQueryProxyUrl(deps.processApi?.env ?? {});
      if (!baseUrl) {
        return {
          content: [
            { type: "text", text: "Image gateway is not available in the current session." },
          ],
          details: { ok: false, error: "missing image gateway env" },
        };
      }

      const label = taskIds.length === 1 ? taskIds[0] : `${taskIds.length} 个任务`;
      onUpdate?.({
        content: [{ type: "text", text: `正在异步查询图片生成任务 ${label}...` }],
        details: { taskIds, status: "querying" },
      });

      const settled = await Promise.all(
        taskIds.map((id) => pollSingleTask(id, baseUrl, signal, deps, onUpdate, ctx)),
      );

      const content = [];
      const details = { results: [] };
      let allOk = true;

      for (const result of settled) {
        details.results.push(result);

        if (result.ok) {
          const summary = [
            `任务 ${result.taskId} 已完成！`,
            `生成了 ${result.images.length} 张图片。`,
            `保存到: ${result.savedPaths.join(", ")}`,
          ];
          content.push({ type: "text", text: summary.join(" ") });
          for (const image of result.images) {
            content.push({
              type: "image",
              data: image.dataBase64,
              mimeType: image.mimeType || "image/png",
            });
          }
        } else {
          allOk = false;
          const errorLabel =
            result.status === "failed"
              ? "失败"
              : result.status === "cancelled"
                ? "已取消"
                : result.status === "timeout"
                  ? "查询超时"
                  : "出错";
          content.push({
            type: "text",
            text: `任务 ${result.taskId} ${errorLabel}: ${result.error || "未知"}`,
          });
        }
      }

      return { content, details: { ok: allOk, ...details } };
    },
  };
}

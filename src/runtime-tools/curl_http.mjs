async function makeTempPath(prefix, deps, tempArtifactTracker) {
  const targetPath = deps.pathApi.join(
    deps.osApi.tmpdir(),
    `${prefix}-${Date.now()}-${deps.cryptoApi.randomBytes(6).toString("hex")}.tmp`,
  );
  tempArtifactTracker.register(targetPath);
  return targetPath;
}

export async function fetchUrlWithCurl(url, options, deps, tempArtifactTracker) {
  const headersPath = await makeTempPath("web-curl-headers", deps, tempArtifactTracker);
  const bodyPath = await makeTempPath("web-curl-body", deps, tempArtifactTracker);
  const args = [
    "-sS",
    "-L",
    "--compressed",
    "-D",
    headersPath,
    "-o",
    bodyPath,
    "-w",
    "__NC_CURL_META__%{http_code}\t%{content_type}\t%{url_effective}",
  ];

  if (options?.userAgent) {
    args.push("-A", options.userAgent);
  }
  if (Array.isArray(options?.headers)) {
    for (const header of options.headers) {
      if (typeof header === "string" && header.trim()) {
        args.push("-H", header.trim());
      }
    }
  }

  args.push(url);

  const execResult = await deps.execFileImpl("curl", args, {
    encoding: "utf8",
    maxBuffer: 20 * 1024 * 1024,
    signal: undefined,
  });
  const metaLine = String(execResult.stdout ?? "").trim();
  if (!metaLine.startsWith("__NC_CURL_META__")) {
    throw new Error(`Unexpected curl metadata output: ${metaLine || "<empty>"}`);
  }

  const [, rawStatus = "", rawContentType = "", rawFinalUrl = ""] = metaLine.match(
    /^__NC_CURL_META__(\d*)\t([^\t]*)\t([\s\S]*)$/,
  ) ?? [];

  return {
    status: Number(rawStatus || 0),
    contentType: normalizeOptionalString(rawContentType) ?? null,
    finalUrl: normalizeOptionalString(rawFinalUrl) ?? url,
    headersText: await deps.fsPromises.readFile(headersPath, "utf8"),
    bodyBuffer: await deps.fsPromises.readFile(bodyPath),
    transport: "curl",
  };
}

function normalizeOptionalString(value) {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

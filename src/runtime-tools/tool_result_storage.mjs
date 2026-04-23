const WORKDIR_RESULT_DIR = ".nineclaw-tool-results";
const PROCESS_EXIT_EVENTS = ["exit", "SIGINT", "SIGTERM", "uncaughtException"];

export function createTempArtifactTracker(deps) {
  const files = new Set();
  let bound = false;

  function cleanup() {
    for (const filePath of files) {
      try {
        deps.fsSync.rmSync(filePath, { force: true });
      } catch {}
    }
    files.clear();
  }

  return {
    register(filePath) {
      files.add(filePath);
      if (bound) {
        return;
      }
      bound = true;
      for (const eventName of PROCESS_EXIT_EVENTS) {
        deps.processApi.once(eventName, cleanup);
      }
    },
  };
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

export async function persistLargeTextResult(text, ctx, options, deps) {
  const resultDirInWorkdir = deps.pathApi.resolve(ctx.cwd || ".", WORKDIR_RESULT_DIR);
  const fallbackDir = deps.pathApi.join(deps.osApi.tmpdir(), WORKDIR_RESULT_DIR);
  const targetDir = (await isWritableDirectory(resultDirInWorkdir, deps))
    ? resultDirInWorkdir
    : fallbackDir;

  await deps.fsPromises.mkdir(targetDir, { recursive: true });
  const targetPath = deps.pathApi.join(
    targetDir,
    `${options.filePrefix}-${Date.now()}-${deps.cryptoApi.randomBytes(6).toString("hex")}.txt`,
  );

  await deps.withFileMutationQueue(targetPath, async () => {
    await deps.fsPromises.writeFile(targetPath, text, "utf8");
  });

  options.tempArtifactTracker.register(targetPath);
  return targetPath;
}

export async function finalizeLargeTextResult(text, ctx, options, deps) {
  if (text.length <= options.maxResultSizeChars) {
    return {
      inlineText: text,
      storage: { inline: true },
    };
  }

  const filePath = await persistLargeTextResult(text, ctx, options, deps);
  return {
    inlineText:
      `Result exceeded ${options.maxResultSizeChars} characters and was written to ${filePath}.\n` +
      "Use the file reading tool to inspect the full output.",
    storage: {
      inline: false,
      filePath,
      sizeChars: text.length,
    },
  };
}

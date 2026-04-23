use std::fs;
use std::path::{Path, PathBuf};

const MANAGED_RUNTIME_EXTENSION_FILE: &str = "nineclaw-managed-runtime.mjs";
const MANAGED_RUNTIME_WEB_SEARCH_FILE: &str = "nineclaw-web-search-tool.mjs";
const MANAGED_RUNTIME_WEB_FETCH_FILE: &str = "nineclaw-web-fetch-tool.mjs";
const MANAGED_RUNTIME_TOOL_RESULT_STORAGE_FILE: &str = "tool_result_storage.mjs";
const MANAGED_RUNTIME_CURL_HTTP_FILE: &str = "curl_http.mjs";
const MANAGED_RUNTIME_WEB_SEARCH_TRANSPORT_FILE: &str = "web_search_transport.mjs";
const WEB_SEARCH_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/web_search_tool.mjs");
const WEB_FETCH_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/web_fetch_tool.mjs");
const TOOL_RESULT_STORAGE_SOURCE: &str =
    include_str!("../../src/runtime-tools/tool_result_storage.mjs");
const CURL_HTTP_SOURCE: &str = include_str!("../../src/runtime-tools/curl_http.mjs");
const WEB_SEARCH_TRANSPORT_SOURCE: &str =
    include_str!("../../src/runtime-tools/web_search_transport.mjs");

pub(crate) fn write_managed_runtime_extension_files(
    runtime_dir: &Path,
    typebox_import_path: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(runtime_dir).map_err(|error| {
        format!(
            "创建 managed runtime 目录失败 {}: {error}",
            runtime_dir.display()
        )
    })?;

    let helper_path = runtime_dir.join(MANAGED_RUNTIME_WEB_SEARCH_FILE);
    fs::write(&helper_path, WEB_SEARCH_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 web_search 运行时模块失败 {}: {error}",
            helper_path.display()
        )
    })?;

    let fetch_helper_path = runtime_dir.join(MANAGED_RUNTIME_WEB_FETCH_FILE);
    fs::write(&fetch_helper_path, WEB_FETCH_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 web_fetch 运行时模块失败 {}: {error}",
            fetch_helper_path.display()
        )
    })?;

    let storage_helper_path = runtime_dir.join(MANAGED_RUNTIME_TOOL_RESULT_STORAGE_FILE);
    fs::write(&storage_helper_path, TOOL_RESULT_STORAGE_SOURCE).map_err(|error| {
        format!(
            "写入 tool result storage 运行时模块失败 {}: {error}",
            storage_helper_path.display()
        )
    })?;

    let curl_http_path = runtime_dir.join(MANAGED_RUNTIME_CURL_HTTP_FILE);
    fs::write(&curl_http_path, CURL_HTTP_SOURCE).map_err(|error| {
        format!(
            "写入 curl_http 运行时模块失败 {}: {error}",
            curl_http_path.display()
        )
    })?;

    let web_search_transport_path = runtime_dir.join(MANAGED_RUNTIME_WEB_SEARCH_TRANSPORT_FILE);
    fs::write(&web_search_transport_path, WEB_SEARCH_TRANSPORT_SOURCE).map_err(|error| {
        format!(
            "写入 web_search_transport 运行时模块失败 {}: {error}",
            web_search_transport_path.display()
        )
    })?;

    let extension_source = build_managed_runtime_extension_source(typebox_import_path)?;
    let extension_path = runtime_dir.join(MANAGED_RUNTIME_EXTENSION_FILE);
    fs::write(&extension_path, extension_source).map_err(|error| {
        format!(
            "写入 managed runtime 扩展失败 {}: {error}",
            extension_path.display()
        )
    })?;

    Ok(extension_path)
}

fn build_managed_runtime_extension_source(typebox_import_path: &Path) -> Result<String, String> {
    let import_path = serde_json::to_string(&typebox_import_path.to_string_lossy())
        .map_err(|error| format!("序列化 typebox 路径失败: {error}"))?;
    Ok(format!(
        r#"import fs from "node:fs";
import * as fsPromises from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";
import * as crypto from "node:crypto";
import {{ execFile as execFileCallback }} from "node:child_process";
import {{ promisify }} from "node:util";
import {{ Type }} from {import_path};
import {{
  formatSize,
  truncateHead,
  withFileMutationQueue,
}} from "@mariozechner/pi-coding-agent";
import {{ createWebSearchTool }} from "./{MANAGED_RUNTIME_WEB_SEARCH_FILE}";
import {{ createWebFetchTool }} from "./{MANAGED_RUNTIME_WEB_FETCH_FILE}";

const execFile = promisify(execFileCallback);

const HTTP_PARAMS = Type.Object({{
  alias: Type.String({{ description: "Configured external API alias" }}),
  method: Type.String({{ description: "HTTP method, e.g. GET/POST" }}),
  path: Type.Optional(Type.String({{ description: "Path appended to the configured base URL" }})),
  query: Type.Optional(Type.Record(Type.String(), Type.String())),
  headers: Type.Optional(Type.Record(Type.String(), Type.String())),
  body: Type.Optional(Type.String({{ description: "Optional raw request body" }}))
}});

function readHarness() {{
  const file = process.env.NINECLAW_HARNESS_FILE?.trim();
  if (!file) return {{}};
  try {{
    return JSON.parse(fs.readFileSync(file, "utf8"));
  }} catch {{
    return {{}};
  }}
}}

function applyHarness(pi, harness) {{
  const active = Array.isArray(harness.activeTools) ? harness.activeTools.filter(Boolean) : [];
  if (harness.enableExternalApiProxy) {{
    active.push("nineclaw_external_api");
  }}
  if (active.length > 0) {{
    const unique = [...new Set(active)];
    pi.setActiveTools(unique);
  }}
}}

function proxyUrl() {{
  const base = process.env.NINECLAW_PROXY_BASE_URL?.trim();
  const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
  if (!base || !token) return null;
  return `${{base}}/external/${{token}}/dispatch`;
}}

export default function(pi) {{
  let externalToolRegistered = false;
  let webSearchToolRegistered = false;
  let webFetchToolRegistered = false;

  function ensureWebSearchTool() {{
    if (webSearchToolRegistered) return;
    webSearchToolRegistered = true;
    pi.registerTool(
      createWebSearchTool({{
        Type,
        fetchImpl: fetch,
        execFileImpl: execFile,
        fsPromises,
        fsConstants: fs.constants,
        fsSync: fs,
        pathApi: path,
        osApi: os,
        cryptoApi: crypto,
        processApi: process,
        withFileMutationQueue,
        truncateHead,
        formatSize,
      }})
    );
  }}

  function ensureWebFetchTool() {{
    if (webFetchToolRegistered) return;
    webFetchToolRegistered = true;
    pi.registerTool(
      createWebFetchTool({{
        Type,
        execFileImpl: execFile,
        fsPromises,
        fsConstants: fs.constants,
        fsSync: fs,
        pathApi: path,
        osApi: os,
        cryptoApi: crypto,
        processApi: process,
        withFileMutationQueue,
      }})
    );
  }}

  function ensureExternalTool() {{
    if (externalToolRegistered) return;
    externalToolRegistered = true;
    pi.registerTool({{
      name: "nineclaw_external_api",
      label: "NineClaw External API Proxy",
      description: "Call an external HTTP API through NineClaw's credential proxy.",
      promptSnippet: "Call configured external APIs without exposing real credentials.",
      promptGuidelines: [
        "Use this tool for authenticated external HTTP APIs instead of embedding API keys in bash/curl."
      ],
      parameters: HTTP_PARAMS,
      async execute(_toolCallId, params) {{
        const target = proxyUrl();
        if (!target) {{
          return {{
            content: [{{ type: "text", text: "Credential proxy is not available for this session." }}],
            details: {{}},
          }};
        }}

        const response = await fetch(target, {{
          method: "POST",
          headers: {{ "content-type": "application/json" }},
          body: JSON.stringify(params ?? {{}})
        }});
        const text = await response.text();
        return {{
          content: [{{ type: "text", text }}],
          details: {{
            status: response.status
          }}
        }};
      }}
    }});
  }}

  pi.on("session_start", async () => {{
    const harness = readHarness();
    ensureWebSearchTool();
    ensureWebFetchTool();
    if (harness.enableExternalApiProxy) {{
      ensureExternalTool();
    }}
    applyHarness(pi, harness);
  }});

  pi.on("before_agent_start", async (event) => {{
    const harness = readHarness();
    ensureWebSearchTool();
    ensureWebFetchTool();
    if (harness.enableExternalApiProxy) {{
      ensureExternalTool();
    }}
    applyHarness(pi, harness);
    const promptAppend = typeof harness.promptAppend === "string" ? harness.promptAppend.trim() : "";
    if (!promptAppend) {{
      return undefined;
    }}
    return {{
      systemPrompt: `${{event.systemPrompt}}\n\n# Active Harness\n\n${{promptAppend}}`
    }};
  }});

  pi.on("tool_call", async (event) => {{
    if (event.toolName !== "bash") {{
      return undefined;
    }}
    const command = String(event.input?.command ?? "");
    if (!command.trim()) {{
      return undefined;
    }}
    if (/(authorization\\s*:\\s*bearer|x-api-key|api[_-]?key\\s*=|token\\s*=)/i.test(command)) {{
      return {{
        block: true,
        reason: "Detected inline credential usage in bash. Use nineclaw_external_api or the credential proxy instead."
      }};
    }}
    return undefined;
  }});
}}
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_source_registers_web_tools_and_support_files() {
        let runtime_dir = std::env::temp_dir().join("nineclaw-managed-runtime-ext-test");
        let typebox_path = PathBuf::from("/tmp/typebox/index.mjs");
        let extension_path = write_managed_runtime_extension_files(&runtime_dir, &typebox_path)
            .expect("write files");

        let helper_path = runtime_dir.join(MANAGED_RUNTIME_WEB_SEARCH_FILE);
        let fetch_helper_path = runtime_dir.join(MANAGED_RUNTIME_WEB_FETCH_FILE);
        let storage_helper_path = runtime_dir.join(MANAGED_RUNTIME_TOOL_RESULT_STORAGE_FILE);
        let curl_http_path = runtime_dir.join(MANAGED_RUNTIME_CURL_HTTP_FILE);
        let web_search_transport_path = runtime_dir.join(MANAGED_RUNTIME_WEB_SEARCH_TRANSPORT_FILE);
        let extension_source = fs::read_to_string(&extension_path).expect("read extension");
        let helper_source = fs::read_to_string(&helper_path).expect("read helper");
        let fetch_helper_source = fs::read_to_string(&fetch_helper_path).expect("read fetch helper");
        let storage_helper_source =
            fs::read_to_string(&storage_helper_path).expect("read storage helper");
        let curl_http_source = fs::read_to_string(&curl_http_path).expect("read curl http helper");
        let web_search_transport_source =
            fs::read_to_string(&web_search_transport_path).expect("read search transport helper");

        assert!(extension_source.contains("ensureWebSearchTool"));
        assert!(extension_source.contains("ensureWebFetchTool"));
        assert!(extension_source.contains("createWebSearchTool"));
        assert!(extension_source.contains("createWebFetchTool"));
        assert!(helper_source.contains("name: \"web_search\""));
        assert!(fetch_helper_source.contains("name: \"web_fetch\""));
        assert!(helper_source.contains("tool_result_storage.mjs"));
        assert!(fetch_helper_source.contains("tool_result_storage.mjs"));
        assert!(fetch_helper_source.contains("curl_http.mjs"));
        assert!(helper_source.contains("web_search_transport.mjs"));
        assert!(storage_helper_source.contains("Result exceeded"));
        assert!(storage_helper_source.contains("Use the file reading tool"));
        assert!(curl_http_source.contains("execFileImpl(\"curl\""));
        assert!(web_search_transport_source.contains("anti-bot / enablejs"));
        assert!(web_search_transport_source.contains("curl_http.mjs"));

        let _ = fs::remove_file(extension_path);
        let _ = fs::remove_file(helper_path);
        let _ = fs::remove_file(fetch_helper_path);
        let _ = fs::remove_file(storage_helper_path);
        let _ = fs::remove_file(curl_http_path);
        let _ = fs::remove_file(web_search_transport_path);
        let _ = fs::remove_dir_all(runtime_dir);
    }
}

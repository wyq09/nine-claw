use std::fs;
use std::path::{Path, PathBuf};

const MANAGED_RUNTIME_EXTENSION_FILE: &str = "nineclaw-managed-runtime.mjs";
const MANAGED_RUNTIME_WEB_SEARCH_FILE: &str = "nineclaw-web-search-tool.mjs";
const MANAGED_RUNTIME_WEB_FETCH_FILE: &str = "nineclaw-web-fetch-tool.mjs";
const MANAGED_RUNTIME_IMAGE_GENERATION_FILE: &str = "nineclaw-image-generation-tool.mjs";
const MANAGED_RUNTIME_IMAGE_TASK_QUERY_FILE: &str = "nineclaw-image-task-query-tool.mjs";
const MANAGED_RUNTIME_TOOL_RESULT_STORAGE_FILE: &str = "tool_result_storage.mjs";
const MANAGED_RUNTIME_CURL_HTTP_FILE: &str = "curl_http.mjs";
const MANAGED_RUNTIME_WEB_SEARCH_TRANSPORT_FILE: &str = "web_search_transport.mjs";
const MANAGED_RUNTIME_IMAGE_DOWNLOADER_FILE: &str = "image_downloader.mjs";
const MANAGED_RUNTIME_AGENT_DELEGATE_FILE: &str = "nineclaw-agent-delegate-tool.mjs";
const MANAGED_RUNTIME_ASK_USER_FILE: &str = "nineclaw-ask-user-tool.mjs";
const MANAGED_RUNTIME_MCP_TOOL_FILE: &str = "nineclaw-mcp-tool.mjs";
const MANAGED_RUNTIME_MCP_CONFIG_FILE: &str = "nineclaw-mcp-config-tool.mjs";
const MANAGED_RUNTIME_MEMORY_UPDATE_FILE: &str = "memory_update_tool.mjs";
const MANAGED_RUNTIME_MEMORY_SEARCH_FILE: &str = "memory_search_tool.mjs";
const MANAGED_RUNTIME_MEMORY_READ_FILE: &str = "memory_read_tool.mjs";
const MANAGED_RUNTIME_MEMORY_DELETE_FILE: &str = "memory_delete_tool.mjs";
const MANAGED_RUNTIME_MEMORY_STORE_FILE: &str = "memory_store_tool.mjs";
const MANAGED_RUNTIME_MEMORY_SAVE_FILE: &str = "memory_save_tool.mjs";
const MANAGED_RUNTIME_MEMORY_GET_FILE: &str = "memory_get_tool.mjs";
const MANAGED_RUNTIME_MEMORY_FORGET_FILE: &str = "memory_forget_tool.mjs";
const MANAGED_RUNTIME_MEMORY_LIST_FILE: &str = "memory_list_tool.mjs";
const MANAGED_RUNTIME_CHAT_SEARCH_FILE: &str = "chat_search_tool.mjs";
const MANAGED_RUNTIME_MEMORY_TOOL_TRANSPORT_FILE: &str = "memory_tool_transport.mjs";
const MANAGED_RUNTIME_CREATE_TASK_FILE: &str = "create_scheduled_task.mjs";
const MANAGED_RUNTIME_QUERY_TASK_FILE: &str = "query_scheduled_task.mjs";
const MANAGED_RUNTIME_QUERY_TASK_INFO_FILE: &str = "query_scheduled_task_info.mjs";
const MANAGED_RUNTIME_SKILL_CREATOR_FILE: &str = "skill_creator_tool.mjs";
const MANAGED_RUNTIME_SKILL_EVOLUTION_FILE: &str = "skill_evolution_runtime.mjs";
const MANAGED_RUNTIME_SKILL_AUTO_CREATION_PROMPT_FILE: &str = "skill_auto_creation.md";
const MANAGED_RUNTIME_SKILL_REFLECTION_PROMPT_FILE: &str = "skill_reflection.md";
const WEB_SEARCH_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/web_search_tool.mjs");
const WEB_FETCH_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/web_fetch_tool.mjs");
const IMAGE_GENERATION_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/image_generation_tool.mjs");
const IMAGE_TASK_QUERY_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/image_task_query_tool.mjs");
const TOOL_RESULT_STORAGE_SOURCE: &str =
    include_str!("../../src/runtime-tools/tool_result_storage.mjs");
const CURL_HTTP_SOURCE: &str = include_str!("../../src/runtime-tools/curl_http.mjs");
const WEB_SEARCH_TRANSPORT_SOURCE: &str =
    include_str!("../../src/runtime-tools/web_search_transport.mjs");
const IMAGE_DOWNLOADER_SOURCE: &str = include_str!("../../src/runtime-tools/image_downloader.mjs");
const AGENT_DELEGATE_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/agent_delegate_tool.mjs");
const ASK_USER_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/ask_user_tool.mjs");
const MCP_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/mcp_tool.mjs");
const MCP_CONFIG_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/mcp_config.mjs");
const MEMORY_UPDATE_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/memory_update_tool.mjs");
const MEMORY_SEARCH_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/memory_search_tool.mjs");
const MEMORY_READ_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_read_tool.mjs");
const MEMORY_DELETE_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/memory_delete_tool.mjs");
const MEMORY_STORE_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/memory_store_tool.mjs");
const MEMORY_SAVE_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_save_tool.mjs");
const MEMORY_GET_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_get_tool.mjs");
const MEMORY_FORGET_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/memory_forget_tool.mjs");
const MEMORY_LIST_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_list_tool.mjs");
const CHAT_SEARCH_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/chat_search_tool.mjs");
const MEMORY_TOOL_TRANSPORT_SOURCE: &str =
    include_str!("../../src/runtime-tools/memory_tool_transport.mjs");
const CREATE_SCHEDULED_TASK_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/create_scheduled_task.mjs");
const QUERY_SCHEDULED_TASK_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/query_scheduled_task.mjs");
const QUERY_SCHEDULED_TASK_INFO_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/query_scheduled_task_info.mjs");
const SKILL_CREATOR_TOOL_SOURCE: &str =
    include_str!("../../src/runtime-tools/skill_creator_tool.mjs");
const SKILL_EVOLUTION_RUNTIME_SOURCE: &str =
    include_str!("../../src/runtime-tools/skill_evolution_runtime.mjs");
const SKILL_AUTO_CREATION_PROMPT_SOURCE: &str =
    include_str!("../../src/runtime-tools/prompts/skill_auto_creation.md");
const SKILL_REFLECTION_PROMPT_SOURCE: &str =
    include_str!("../../src/runtime-tools/prompts/skill_reflection.md");

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

    let image_downloader_path = runtime_dir.join(MANAGED_RUNTIME_IMAGE_DOWNLOADER_FILE);
    fs::write(&image_downloader_path, IMAGE_DOWNLOADER_SOURCE).map_err(|error| {
        format!(
            "写入 image_downloader 运行时模块失败 {}: {error}",
            image_downloader_path.display()
        )
    })?;

    let image_generation_path = runtime_dir.join(MANAGED_RUNTIME_IMAGE_GENERATION_FILE);
    fs::write(&image_generation_path, IMAGE_GENERATION_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 image_generation 运行时模块失败 {}: {error}",
            image_generation_path.display()
        )
    })?;

    let image_task_query_path = runtime_dir.join(MANAGED_RUNTIME_IMAGE_TASK_QUERY_FILE);
    fs::write(&image_task_query_path, IMAGE_TASK_QUERY_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 image_task_query 运行时模块失败 {}: {error}",
            image_task_query_path.display()
        )
    })?;

    let agent_delegate_path = runtime_dir.join(MANAGED_RUNTIME_AGENT_DELEGATE_FILE);
    fs::write(&agent_delegate_path, AGENT_DELEGATE_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 agent_delegate 运行时模块失败 {}: {error}",
            agent_delegate_path.display()
        )
    })?;

    let ask_user_path = runtime_dir.join(MANAGED_RUNTIME_ASK_USER_FILE);
    fs::write(&ask_user_path, ASK_USER_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 ask_user 运行时模块失败 {}: {error}",
            ask_user_path.display()
        )
    })?;

    let mcp_tool_path = runtime_dir.join(MANAGED_RUNTIME_MCP_TOOL_FILE);
    fs::write(&mcp_tool_path, MCP_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 mcp_tool 运行时模块失败 {}: {error}",
            mcp_tool_path.display()
        )
    })?;

    let mcp_config_path = runtime_dir.join(MANAGED_RUNTIME_MCP_CONFIG_FILE);
    fs::write(&mcp_config_path, MCP_CONFIG_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 mcp_config 运行时模块失败 {}: {error}",
            mcp_config_path.display()
        )
    })?;

    let memory_update_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_UPDATE_FILE);
    fs::write(&memory_update_path, MEMORY_UPDATE_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_update 运行时模块失败 {}: {error}",
            memory_update_path.display()
        )
    })?;

    let memory_search_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_SEARCH_FILE);
    fs::write(&memory_search_path, MEMORY_SEARCH_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_search 运行时模块失败 {}: {error}",
            memory_search_path.display()
        )
    })?;

    let memory_read_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_READ_FILE);
    fs::write(&memory_read_path, MEMORY_READ_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_read 运行时模块失败 {}: {error}",
            memory_read_path.display()
        )
    })?;

    let memory_delete_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_DELETE_FILE);
    fs::write(&memory_delete_path, MEMORY_DELETE_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_delete 运行时模块失败 {}: {error}",
            memory_delete_path.display()
        )
    })?;

    let memory_store_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_STORE_FILE);
    fs::write(&memory_store_path, MEMORY_STORE_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_store 运行时模块失败 {}: {error}",
            memory_store_path.display()
        )
    })?;

    let memory_save_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_SAVE_FILE);
    fs::write(&memory_save_path, MEMORY_SAVE_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_save 运行时模块失败 {}: {error}",
            memory_save_path.display()
        )
    })?;

    let memory_get_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_GET_FILE);
    fs::write(&memory_get_path, MEMORY_GET_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_get 运行时模块失败 {}: {error}",
            memory_get_path.display()
        )
    })?;

    let memory_forget_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_FORGET_FILE);
    fs::write(&memory_forget_path, MEMORY_FORGET_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_forget 运行时模块失败 {}: {error}",
            memory_forget_path.display()
        )
    })?;

    let memory_list_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_LIST_FILE);
    fs::write(&memory_list_path, MEMORY_LIST_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 memory_list 运行时模块失败 {}: {error}",
            memory_list_path.display()
        )
    })?;

    let chat_search_path = runtime_dir.join(MANAGED_RUNTIME_CHAT_SEARCH_FILE);
    fs::write(&chat_search_path, CHAT_SEARCH_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 chat_search 运行时模块失败 {}: {error}",
            chat_search_path.display()
        )
    })?;

    let memory_tool_transport_path = runtime_dir.join(MANAGED_RUNTIME_MEMORY_TOOL_TRANSPORT_FILE);
    fs::write(&memory_tool_transport_path, MEMORY_TOOL_TRANSPORT_SOURCE).map_err(|error| {
        format!(
            "写入 memory_tool_transport 运行时模块失败 {}: {error}",
            memory_tool_transport_path.display()
        )
    })?;

    let create_task_path = runtime_dir.join(MANAGED_RUNTIME_CREATE_TASK_FILE);
    fs::write(&create_task_path, CREATE_SCHEDULED_TASK_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 create_scheduled_task 运行时模块失败 {}: {error}",
            create_task_path.display()
        )
    })?;

    let query_task_path = runtime_dir.join(MANAGED_RUNTIME_QUERY_TASK_FILE);
    fs::write(&query_task_path, QUERY_SCHEDULED_TASK_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 query_scheduled_task 运行时模块失败 {}: {error}",
            query_task_path.display()
        )
    })?;

    let query_task_info_path = runtime_dir.join(MANAGED_RUNTIME_QUERY_TASK_INFO_FILE);
    fs::write(&query_task_info_path, QUERY_SCHEDULED_TASK_INFO_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 query_scheduled_task_info 运行时模块失败 {}: {error}",
            query_task_info_path.display()
        )
    })?;

    let skill_creator_path = runtime_dir.join(MANAGED_RUNTIME_SKILL_CREATOR_FILE);
    fs::write(&skill_creator_path, SKILL_CREATOR_TOOL_SOURCE).map_err(|error| {
        format!(
            "写入 skill_creator 运行时模块失败 {}: {error}",
            skill_creator_path.display()
        )
    })?;

    let skill_evolution_path = runtime_dir.join(MANAGED_RUNTIME_SKILL_EVOLUTION_FILE);
    fs::write(&skill_evolution_path, SKILL_EVOLUTION_RUNTIME_SOURCE).map_err(|error| {
        format!(
            "写入 skill_evolution 运行时模块失败 {}: {error}",
            skill_evolution_path.display()
        )
    })?;

    let skill_auto_prompt_path = runtime_dir.join(MANAGED_RUNTIME_SKILL_AUTO_CREATION_PROMPT_FILE);
    fs::write(&skill_auto_prompt_path, SKILL_AUTO_CREATION_PROMPT_SOURCE).map_err(|error| {
        format!(
            "写入 skill auto-creation prompt 失败 {}: {error}",
            skill_auto_prompt_path.display()
        )
    })?;

    let skill_reflection_prompt_path =
        runtime_dir.join(MANAGED_RUNTIME_SKILL_REFLECTION_PROMPT_FILE);
    fs::write(
        &skill_reflection_prompt_path,
        SKILL_REFLECTION_PROMPT_SOURCE,
    )
    .map_err(|error| {
        format!(
            "写入 skill reflection prompt 失败 {}: {error}",
            skill_reflection_prompt_path.display()
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
import http from "node:http";
import {{ execFile as execFileCallback }} from "node:child_process";
import {{ promisify }} from "node:util";
import {{ fileURLToPath }} from "node:url";
import {{ Type }} from {import_path};
import {{
  formatSize,
  truncateHead,
  withFileMutationQueue,
}} from "@mariozechner/pi-coding-agent";
import {{ createWebSearchTool }} from "./{MANAGED_RUNTIME_WEB_SEARCH_FILE}";
import {{ createWebFetchTool }} from "./{MANAGED_RUNTIME_WEB_FETCH_FILE}";
import {{ createImageGenerationTool }} from "./{MANAGED_RUNTIME_IMAGE_GENERATION_FILE}";
import {{ createImageTaskQueryTool }} from "./{MANAGED_RUNTIME_IMAGE_TASK_QUERY_FILE}";
import {{ createAgentDelegateTool }} from "./{MANAGED_RUNTIME_AGENT_DELEGATE_FILE}";
import {{ createAskUserTool }} from "./{MANAGED_RUNTIME_ASK_USER_FILE}";
import {{ createMcpTool }} from "./{MANAGED_RUNTIME_MCP_TOOL_FILE}";
import {{ createMcpConfigTool }} from "./{MANAGED_RUNTIME_MCP_CONFIG_FILE}";
import {{ createMemoryUpdateTool }} from "./{MANAGED_RUNTIME_MEMORY_UPDATE_FILE}";
import {{ createMemorySearchTool }} from "./{MANAGED_RUNTIME_MEMORY_SEARCH_FILE}";
import {{ createMemoryReadTool }} from "./{MANAGED_RUNTIME_MEMORY_READ_FILE}";
import {{ createMemoryDeleteTool }} from "./{MANAGED_RUNTIME_MEMORY_DELETE_FILE}";
import {{ createMemoryStoreTool }} from "./{MANAGED_RUNTIME_MEMORY_STORE_FILE}";
import {{ createMemorySaveTool }} from "./{MANAGED_RUNTIME_MEMORY_SAVE_FILE}";
import {{ createMemoryGetTool }} from "./{MANAGED_RUNTIME_MEMORY_GET_FILE}";
import {{ createMemoryForgetTool }} from "./{MANAGED_RUNTIME_MEMORY_FORGET_FILE}";
import {{ createMemoryListTool }} from "./{MANAGED_RUNTIME_MEMORY_LIST_FILE}";
import {{ createChatSearchTool }} from "./{MANAGED_RUNTIME_CHAT_SEARCH_FILE}";
import {{ createCreateScheduledTaskTool }} from "./{MANAGED_RUNTIME_CREATE_TASK_FILE}";
import {{ createQueryScheduledTaskTool }} from "./{MANAGED_RUNTIME_QUERY_TASK_FILE}";
import {{ createQueryScheduledTaskInfoTool }} from "./{MANAGED_RUNTIME_QUERY_TASK_INFO_FILE}";
import {{ createSkillCreatorTool }} from "./{MANAGED_RUNTIME_SKILL_CREATOR_FILE}";
import {{ createSkillEvolutionRuntime }} from "./{MANAGED_RUNTIME_SKILL_EVOLUTION_FILE}";

const execFile = promisify(execFileCallback);

// node:http based POST for localhost proxy calls — bypasses undici fetch
// which can emit UND_ERR_SOCKET on some Node.js builds.
function localHttpPost(url, options) {{
  return new Promise((resolve, reject) => {{
    const urlObj = new URL(url);
    const body = options?.body;
    const headers = {{ ...(options?.headers || {{}}) }};
    if (body != null && !headers["content-length"] && !headers["Content-Length"]) {{
      headers["content-length"] = String(Buffer.byteLength(body));
    }}
    const req = http.request({{
      hostname: urlObj.hostname,
      port: urlObj.port,
      path: urlObj.pathname,
      method: options?.method || "POST",
      headers,
    }}, (res) => {{
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => {{
        const text = Buffer.concat(chunks).toString("utf8");
        resolve({{
          ok: res.statusCode >= 200 && res.statusCode < 300,
          status: res.statusCode,
          text: () => Promise.resolve(text),
          json: () => {{ try {{ return Promise.resolve(JSON.parse(text)); }} catch(e) {{ return Promise.reject(e); }} }},
        }});
      }});
    }});
    req.on("error", reject);
    if (options?.signal) {{
      const onAbort = () => req.destroy(Object.assign(new Error("Aborted"), {{ name: "AbortError" }}));
      if (options.signal.aborted) {{ onAbort(); return; }}
      options.signal.addEventListener("abort", onAbort, {{ once: true }});
    }}
    if (body != null) req.write(body);
    req.end();
  }});
}}

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
  const hasExplicitActiveTools = active.length > 0;
  if (hasExplicitActiveTools) {{
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

const TOOL_REPEAT_LIMIT = 3;
const TOOL_LOOP_GUARD_REASON_PREFIX = "[NineClaw loop guard]";
const NINECLAW_SYSTEM_PROMPT_APPEND = [
  "When you use ask_user and the user submits an answer, treat that tool result as clarification, not as the end of the turn.",
  "After a successful ask_user result, continue the task immediately and answer the user using the submitted information.",
  "Do not stop at the tool result unless the user explicitly asked you to only collect the answer.",
].join("\\n");

function stableToolInput(value) {{
  if (Array.isArray(value)) {{
    return value.map((item) => stableToolInput(item));
  }}
  if (value && typeof value === "object") {{
    const out = {{}};
    for (const key of Object.keys(value).sort()) {{
      const item = value[key];
      if (item !== undefined) {{
        out[key] = stableToolInput(item);
      }}
    }}
    return out;
  }}
  return value;
}}

function stableToolSignature(toolName, input) {{
  return `${{String(toolName ?? "")}}\n${{JSON.stringify(stableToolInput(input ?? {{}}))}}`;
}}

function buildSystemPromptAppend(harness) {{
  const sections = [NINECLAW_SYSTEM_PROMPT_APPEND];
  const promptAppend = typeof harness.promptAppend === "string" ? harness.promptAppend.trim() : "";
  if (promptAppend) {{
    sections.push(`# Active Harness\n\n${{promptAppend}}`);
  }}
  return sections.filter(Boolean).join("\n\n");
}}

export default function(pi) {{
  let externalToolRegistered = false;
  let webSearchToolRegistered = false;
  let webFetchToolRegistered = false;
  let imageGenerationToolRegistered = false;
  let imageTaskQueryToolRegistered = false;
  let agentDelegateToolRegistered = false;
  let askUserToolRegistered = false;
  let mcpToolRegistered = false;
  let mcpConfigToolRegistered = false;
  let memoryUpdateToolRegistered = false;
  let memorySearchToolRegistered = false;
  let memoryReadToolRegistered = false;
  let memoryDeleteToolRegistered = false;
  let memoryStoreToolRegistered = false;
  let memorySaveToolRegistered = false;
  let memoryGetToolRegistered = false;
  let memoryForgetToolRegistered = false;
  let memoryListToolRegistered = false;
  let chatSearchToolRegistered = false;
  let createScheduledTaskToolRegistered = false;
  let queryScheduledTaskToolRegistered = false;
  let queryScheduledTaskInfoToolRegistered = false;
  let skillCreatorToolRegistered = false;
  let lastToolSignature = "";
  let repeatedToolSignatureCount = 0;
  const extensionPath = fileURLToPath(import.meta.url);
  const extensionDir = path.dirname(extensionPath);
  const skillEvolution = createSkillEvolutionRuntime({{
    fsSync: fs,
    fsPromises,
    pathApi: path,
    processApi: process,
    consoleApi: console,
    pi,
    extensionPath,
    autoCreationPromptPath: path.join(extensionDir, "{MANAGED_RUNTIME_SKILL_AUTO_CREATION_PROMPT_FILE}"),
    reflectionPromptPath: path.join(extensionDir, "{MANAGED_RUNTIME_SKILL_REFLECTION_PROMPT_FILE}"),
  }});

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

  function ensureImageGenerationTool() {{
    if (imageGenerationToolRegistered) return;
    imageGenerationToolRegistered = true;
    pi.registerTool(
      createImageGenerationTool({{
        Type,
        fetchImpl: localHttpPost,
        fsPromises,
        pathApi: path,
        osApi: os,
        cryptoApi: crypto,
        processApi: process,
        withFileMutationQueue,
      }})
    );
  }}

  function ensureImageTaskQueryTool() {{
    if (imageTaskQueryToolRegistered) return;
    imageTaskQueryToolRegistered = true;
    pi.registerTool(
      createImageTaskQueryTool({{
        Type,
        fetchImpl: localHttpPost,
        fsPromises,
        pathApi: path,
        osApi: os,
        cryptoApi: crypto,
        processApi: process,
        withFileMutationQueue,
      }})
    );
  }}

  function ensureAgentDelegateTool() {{
    if (agentDelegateToolRegistered) return;
    agentDelegateToolRegistered = true;
    pi.registerTool(
      createAgentDelegateTool({{
        Type,
        fetchImpl: localHttpPost,
        processApi: process,
      }})
    );
  }}

  function ensureAskUserTool() {{
    if (askUserToolRegistered) return;
    askUserToolRegistered = true;
    pi.registerTool(
      createAskUserTool({{
        Type,
        fetchImpl: localHttpPost,
        processApi: process,
      }})
    );
  }}

  function ensureMcpTool() {{
    if (mcpToolRegistered) return;
    mcpToolRegistered = true;
    pi.registerTool(
      createMcpTool({{
        Type,
        fsSync: fs,
        fetchImpl: fetch,
        processApi: process,
      }})
    );
  }}

  function ensureMcpConfigTool() {{
    if (mcpConfigToolRegistered) return;
    mcpConfigToolRegistered = true;
    pi.registerTool(
      createMcpConfigTool({{
        Type,
        fsSync: fs,
        fetchImpl: localHttpPost,
        processApi: process,
      }})
    );
  }}

  function ensureMemoryUpdateTool() {{
    if (memoryUpdateToolRegistered) return;
    memoryUpdateToolRegistered = true;
    pi.registerTool(createMemoryUpdateTool({{ Type }}));
  }}

  function ensureMemorySearchTool() {{
    if (memorySearchToolRegistered) return;
    memorySearchToolRegistered = true;
    pi.registerTool(createMemorySearchTool({{ Type }}));
  }}

  function ensureMemoryReadTool() {{
    if (memoryReadToolRegistered) return;
    memoryReadToolRegistered = true;
    pi.registerTool(createMemoryReadTool({{ Type }}));
  }}

  function ensureMemoryDeleteTool() {{
    if (memoryDeleteToolRegistered) return;
    memoryDeleteToolRegistered = true;
    pi.registerTool(createMemoryDeleteTool({{ Type }}));
  }}

  function ensureMemoryStoreTool() {{
    if (memoryStoreToolRegistered) return;
    memoryStoreToolRegistered = true;
    pi.registerTool(createMemoryStoreTool({{ Type }}));
  }}

  function ensureMemorySaveTool() {{
    if (memorySaveToolRegistered) return;
    memorySaveToolRegistered = true;
    pi.registerTool(createMemorySaveTool({{ Type }}));
  }}

  function ensureMemoryGetTool() {{
    if (memoryGetToolRegistered) return;
    memoryGetToolRegistered = true;
    pi.registerTool(createMemoryGetTool({{ Type }}));
  }}

  function ensureMemoryForgetTool() {{
    if (memoryForgetToolRegistered) return;
    memoryForgetToolRegistered = true;
    pi.registerTool(createMemoryForgetTool({{ Type }}));
  }}

  function ensureMemoryListTool() {{
    if (memoryListToolRegistered) return;
    memoryListToolRegistered = true;
    pi.registerTool(createMemoryListTool({{ Type }}));
  }}

  function ensureChatSearchTool() {{
    if (chatSearchToolRegistered) return;
    chatSearchToolRegistered = true;
    pi.registerTool(createChatSearchTool({{ Type, fetchImpl: localHttpPost, processApi: process }}));
  }}

  function ensureCreateScheduledTaskTool() {{
    if (createScheduledTaskToolRegistered) return;
    createScheduledTaskToolRegistered = true;
    pi.registerTool(createCreateScheduledTaskTool({{ Type, fetchImpl: localHttpPost, processApi: process }}));
  }}

  function ensureQueryScheduledTaskTool() {{
    if (queryScheduledTaskToolRegistered) return;
    queryScheduledTaskToolRegistered = true;
    pi.registerTool(createQueryScheduledTaskTool({{ Type, fetchImpl: localHttpPost, processApi: process }}));
  }}

  function ensureQueryScheduledTaskInfoTool() {{
    if (queryScheduledTaskInfoToolRegistered) return;
    queryScheduledTaskInfoToolRegistered = true;
    pi.registerTool(createQueryScheduledTaskInfoTool({{ Type, fetchImpl: localHttpPost, processApi: process }}));
  }}

  function ensureSkillCreatorTool() {{
    if (skillCreatorToolRegistered) return;
    skillCreatorToolRegistered = true;
    pi.registerTool(createSkillCreatorTool({{
      Type,
      fsSync: fs,
      fsPromises,
      pathApi: path,
      processApi: process,
    }}));
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

        const response = await localHttpPost(target, {{
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
    ensureImageGenerationTool();
    ensureImageTaskQueryTool();
    ensureAgentDelegateTool();
    ensureAskUserTool();
    ensureMcpTool();
    ensureMcpConfigTool();
    ensureMemoryUpdateTool();
    ensureMemorySearchTool();
    ensureMemoryReadTool();
    ensureMemoryDeleteTool();
    ensureMemoryStoreTool();
    ensureMemorySaveTool();
    ensureMemoryGetTool();
    ensureMemoryForgetTool();
    ensureMemoryListTool();
    ensureChatSearchTool();
    ensureCreateScheduledTaskTool();
    ensureQueryScheduledTaskTool();
    ensureQueryScheduledTaskInfoTool();
    ensureSkillCreatorTool();
    if (harness.enableExternalApiProxy) {{
      ensureExternalTool();
    }}
    applyHarness(pi, harness);
  }});

  pi.on("before_agent_start", async (event) => {{
    const harness = readHarness();
    ensureWebSearchTool();
    ensureWebFetchTool();
    ensureImageGenerationTool();
    ensureImageTaskQueryTool();
    ensureAgentDelegateTool();
    ensureAskUserTool();
    ensureMcpTool();
    ensureMcpConfigTool();
    ensureMemoryUpdateTool();
    ensureMemorySearchTool();
    ensureMemoryReadTool();
    ensureMemoryDeleteTool();
    ensureMemoryStoreTool();
    ensureMemorySaveTool();
    ensureMemoryGetTool();
    ensureMemoryForgetTool();
    ensureMemoryListTool();
    ensureChatSearchTool();
    ensureCreateScheduledTaskTool();
    ensureQueryScheduledTaskTool();
    ensureQueryScheduledTaskInfoTool();
    ensureSkillCreatorTool();
    if (harness.enableExternalApiProxy) {{
      ensureExternalTool();
    }}
    applyHarness(pi, harness);
    const promptAppend = buildSystemPromptAppend(harness);
    skillEvolution.reset(event.prompt);
    return {{
      systemPrompt: `${{event.systemPrompt}}\n\n${{promptAppend}}`
    }};
  }});

  pi.on("turn_end", async () => {{
    skillEvolution.noteTurnEnd();
  }});

  pi.on("tool_call", async (event) => {{
    skillEvolution.noteToolCall(event);
    const signature = stableToolSignature(event.toolName, event.input);
    if (signature === lastToolSignature) {{
      repeatedToolSignatureCount += 1;
    }} else {{
      lastToolSignature = signature;
      repeatedToolSignatureCount = 1;
    }}
    if (repeatedToolSignatureCount >= TOOL_REPEAT_LIMIT) {{
      return {{
        block: true,
        reason: `${{TOOL_LOOP_GUARD_REASON_PREFIX}} 检测到连续 ${{TOOL_REPEAT_LIMIT}} 次调用相同工具且参数一致，已拦截本次操作，防止进入死循环。工具：${{String(event.toolName ?? "unknown_tool")}}`
      }};
    }}

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

  pi.on("agent_end", async (event, ctx) => {{
    await skillEvolution.maybeRunAfterAgent(event, ctx);
  }});
}}
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).expect("read generated file")
    }

    fn assert_contains_all(source: &str, needles: &[&str]) {
        for needle in needles {
            assert!(source.contains(needle), "missing {needle}");
        }
    }

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
        let image_downloader_path = runtime_dir.join(MANAGED_RUNTIME_IMAGE_DOWNLOADER_FILE);
        let image_task_query_path = runtime_dir.join(MANAGED_RUNTIME_IMAGE_TASK_QUERY_FILE);
        let image_generation_path = runtime_dir.join(MANAGED_RUNTIME_IMAGE_GENERATION_FILE);
        let mcp_tool_path = runtime_dir.join(MANAGED_RUNTIME_MCP_TOOL_FILE);
        let skill_creator_path = runtime_dir.join(MANAGED_RUNTIME_SKILL_CREATOR_FILE);
        let skill_evolution_path = runtime_dir.join(MANAGED_RUNTIME_SKILL_EVOLUTION_FILE);
        let skill_auto_prompt_path =
            runtime_dir.join(MANAGED_RUNTIME_SKILL_AUTO_CREATION_PROMPT_FILE);
        let skill_reflection_prompt_path =
            runtime_dir.join(MANAGED_RUNTIME_SKILL_REFLECTION_PROMPT_FILE);
        let extension_source = read(&extension_path);
        assert_contains_all(
            &extension_source,
            &[
                "ensureWebSearchTool",
                "ensureWebFetchTool",
                "ensureImageGenerationTool",
                "ensureImageTaskQueryTool",
                "ensureAskUserTool",
                "ensureMcpTool",
                "ensureMcpConfigTool",
                "NINECLAW_SYSTEM_PROMPT_APPEND",
                "After a successful ask_user result, continue the task immediately and answer the user using the submitted information.",
                "buildSystemPromptAppend",
                "const hasExplicitActiveTools = active.length > 0",
                "if (hasExplicitActiveTools)",
                "const TOOL_REPEAT_LIMIT = 3",
                "stableToolSignature(event.toolName, event.input)",
                "[NineClaw loop guard]",
                "import { Type } from \"/tmp/typebox/index.mjs\";",
                "from \"@mariozechner/pi-coding-agent\";",
                "createAgentDelegateTool",
                "createAskUserTool",
                "createMcpTool",
                "createMcpConfigTool",
                "createSkillCreatorTool",
                "createSkillEvolutionRuntime",
                "skillEvolution.reset(event.prompt)",
                "skillEvolution.maybeRunAfterAgent",
            ],
        );
        assert!(read(&helper_path).contains("name: \"web_search\""));
        assert_contains_all(
            &read(&fetch_helper_path),
            &[
                "name: \"web_fetch\"",
                "tool_result_storage.mjs",
                "curl_http.mjs",
                "image_downloader.mjs",
            ],
        );
        assert_contains_all(
            &read(&helper_path),
            &[
                "tool_result_storage.mjs",
                "web_search_transport.mjs",
                "image_downloader.mjs",
            ],
        );
        assert!(read(&image_generation_path).contains("name: \"image_generate\""));
        assert!(read(&image_task_query_path).contains("name: \"image_task_query\""));
        assert!(read(&mcp_tool_path).contains("name: 'mcp_tool'"));
        let mcp_config_path = runtime_dir.join(MANAGED_RUNTIME_MCP_CONFIG_FILE);
        assert!(read(&mcp_config_path).contains("name: 'mcp_config'"));
        assert_contains_all(
            &read(&skill_creator_path),
            &["name: 'skill-creator'", ".agents"],
        );
        assert_contains_all(
            &read(&skill_evolution_path),
            &["NINECLAW_SKILL_EVOLUTION_ENABLED", "--mode', 'json'"],
        );
        assert!(read(&skill_auto_prompt_path).contains("SKILL AUTO-CREATION MODE"));
        assert!(read(&skill_reflection_prompt_path).contains("SKILL REFLECTION MODE"));
        assert_contains_all(
            &read(&image_downloader_path),
            &["fetchUrlWithCurl", "downloadImagesToWorkdir"],
        );
        assert_contains_all(
            &read(&storage_helper_path),
            &["Result exceeded", "Use the file reading tool"],
        );
        assert!(read(&curl_http_path).contains("execFileImpl(\"curl\""));
        assert_contains_all(
            &read(&web_search_transport_path),
            &["anti-bot / enablejs", "curl_http.mjs"],
        );

        let _ = fs::remove_dir_all(runtime_dir);
    }

    #[test]
    fn extension_source_registers_create_scheduled_task_tool() {
        let runtime_dir = std::env::temp_dir().join("nineclaw-create-task-ext-test");
        let typebox_path = PathBuf::from("/tmp/typebox/index.mjs");
        let extension_path = write_managed_runtime_extension_files(&runtime_dir, &typebox_path)
            .expect("write files");

        let tool_path = runtime_dir.join(MANAGED_RUNTIME_CREATE_TASK_FILE);
        let tool_source = fs::read_to_string(&tool_path).expect("read create_scheduled_task");
        let extension_source = fs::read_to_string(&extension_path).expect("read extension");

        assert!(tool_source.contains("create_scheduled_task"));
        assert!(tool_source.contains("createCreateScheduledTaskTool"));
        assert!(extension_source.contains("ensureCreateScheduledTaskTool"));
        assert!(extension_source.contains("createCreateScheduledTaskTool"));

        let _ = fs::remove_file(extension_path);
        let _ = fs::remove_file(tool_path);
        let _ = fs::remove_dir_all(runtime_dir);
    }

    #[test]
    fn extension_source_registers_query_task_tools() {
        let runtime_dir = std::env::temp_dir().join("nineclaw-query-task-ext-test");
        let typebox_path = PathBuf::from("/tmp/typebox/index.mjs");
        let extension_path = write_managed_runtime_extension_files(&runtime_dir, &typebox_path)
            .expect("write files");

        let query_path = runtime_dir.join(MANAGED_RUNTIME_QUERY_TASK_FILE);
        let query_info_path = runtime_dir.join(MANAGED_RUNTIME_QUERY_TASK_INFO_FILE);
        let query_source = fs::read_to_string(&query_path).expect("read query_scheduled_task");
        let query_info_source =
            fs::read_to_string(&query_info_path).expect("read query_scheduled_task_info");
        let extension_source = fs::read_to_string(&extension_path).expect("read extension");

        assert!(query_source.contains("query_scheduled_task"));
        assert!(query_source.contains("createQueryScheduledTaskTool"));
        assert!(query_info_source.contains("query_scheduled_task_info"));
        assert!(query_info_source.contains("createQueryScheduledTaskInfoTool"));
        assert!(extension_source.contains("ensureQueryScheduledTaskTool"));
        assert!(extension_source.contains("ensureQueryScheduledTaskInfoTool"));
        assert!(extension_source.contains("createQueryScheduledTaskTool"));
        assert!(extension_source.contains("createQueryScheduledTaskInfoTool"));

        let _ = fs::remove_file(extension_path);
        let _ = fs::remove_file(query_path);
        let _ = fs::remove_file(query_info_path);
        let _ = fs::remove_dir_all(runtime_dir);
    }
}

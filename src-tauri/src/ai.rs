use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::ipc::Channel;
use uuid::Uuid;

use crate::polish_prompts::{
    polish_mode_label, polish_system_prompt, CLASSIFY_SYSTEM_PROMPT, POLISH_PROCESS_PROMPT,
};
use crate::api_configs;
use crate::commands::DbState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassifyResult {
    pub category: String,
    pub tags: Vec<String>,
    pub summary: String,
    pub recommended_tool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolishProcessStep {
    pub title: String,
    pub desc: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolishStreamEvent {
    Delta { delta: String },
    Done { content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSettings {
    pub config_id: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

/// Anthropic's Messages API rejects requests that omit this header with
/// `400 invalid_request_error: "anthropic-version: header is required"`.
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Anthropic requires an explicit output cap; OpenAI-compatible APIs do not.
const MAX_OUTPUT_TOKENS: u32 = 4096;

/// Ceiling for a whole request. Deliberately generous: a non-streaming polish
/// against a slow model can legitimately take minutes.
const REQUEST_TIMEOUT_SECS: u64 = 300;
/// Abort when the server goes quiet for this long. This — not the total timeout
/// — is what protects against a stalled connection, so it does not cut off a
/// long streaming response that keeps producing tokens.
const READ_TIMEOUT_SECS: u64 = 120;
const CONNECT_TIMEOUT_SECS: u64 = 15;

// Compile-time guard on the timeout budget. This was previously a single 60s
// total timeout, which silently aborted any streaming polish that had not
// finished within a minute. Keep these generous.
const _: () = {
    assert!(
        REQUEST_TIMEOUT_SECS >= 300,
        "total timeout must be wide enough for long responses"
    );
    assert!(
        READ_TIMEOUT_SECS > 60,
        "idle-read timeout must be wider than the old 60s cap"
    );
    assert!(CONNECT_TIMEOUT_SECS < READ_TIMEOUT_SECS);
};

/// Anthropic needs its own endpoint shape, auth scheme and request body, so the
/// provider check lives in one place.
fn is_anthropic_provider(provider: &str) -> bool {
    provider.to_lowercase().contains("anthropic")
}

/// 进程级共享的 HTTP 客户端。
///
/// `reqwest::Client` 内部持有连接池，克隆很廉价；每次请求都新建一个会丢掉连接复用，
/// 让每一次 AI 调用重新走一遍 TCP + TLS 握手。
static HTTP_CLIENT: std::sync::OnceLock<Result<Client, String>> = std::sync::OnceLock::new();

/// 返回共享客户端（首次调用时创建）。
fn build_client() -> Result<Client, String> {
    HTTP_CLIENT.get_or_init(create_client).clone()
}

fn create_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .read_timeout(std::time::Duration::from_secs(READ_TIMEOUT_SECS))
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// Applies provider-appropriate authentication headers.
fn apply_auth_headers(
    request: reqwest::RequestBuilder,
    provider: &str,
    api_key: &str,
) -> reqwest::RequestBuilder {
    if is_anthropic_provider(provider) {
        request
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
    } else {
        request.header("Authorization", format!("Bearer {}", api_key))
    }
}

fn parse_openai_response(response: &Value) -> String {
    response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn parse_anthropic_response(response: &Value) -> String {
    response["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

async fn chat_completion(
    settings: &ApiSettings,
    system_prompt: &str,
    user_message: &str,
) -> Result<String, String> {
    let api_key = settings.api_key.trim();
    if api_key.is_empty() {
        return Err("未找到 API Key，请在设置中重新输入并保存该模型配置".to_string());
    }

    let client = build_client()?;

    let is_anthropic = is_anthropic_provider(&settings.provider);

    let request_body = if is_anthropic {
        serde_json::json!({
            "model": settings.model,
            "max_tokens": MAX_OUTPUT_TOKENS,
            "system": system_prompt,
            "messages": [
                {"role": "user", "content": user_message}
            ]
        })
    } else {
        serde_json::json!({
            "model": settings.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_message}
            ]
        })
    };

    let endpoint = if is_anthropic {
        format!("{}/messages", settings.base_url.trim_end_matches('/'))
    } else {
        format!(
            "{}/chat/completions",
            settings.base_url.trim_end_matches('/')
        )
    };

    let req = apply_auth_headers(
        client
            .post(&endpoint)
            .header("Content-Type", "application/json"),
        &settings.provider,
        api_key,
    );

    let response = req.json(&request_body).send().await.map_err(|e| {
        if e.is_timeout() {
            "模型请求超时，请检查网络后重试".to_string()
        } else if e.is_connect() {
            "模型请求失败，请检查网络或 API 配置".to_string()
        } else {
            format!("模型请求失败，请检查网络或 API 配置: {}", e)
        }
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API 请求失败 ({status}): {body}"));
    }

    let json: Value = response.json().await.map_err(|e| e.to_string())?;

    let content = if is_anthropic {
        parse_anthropic_response(&json)
    } else {
        parse_openai_response(&json)
    };

    if content.is_empty() {
        Err("模型返回了空内容，请重试".to_string())
    } else {
        Ok(content)
    }
}

fn emit_polish_delta(
    on_chunk: &Channel<PolishStreamEvent>,
    delta: String,
) -> Result<(), String> {
    on_chunk
        .send(PolishStreamEvent::Delta { delta })
        .map_err(|e| format!("流式结果发送失败: {}", e))
}

/// OpenAI-compatible servers end the SSE stream with this sentinel. Anthropic
/// does not — its stream just stops after `message_stop`.
const STREAM_DONE_MARKER: &str = "[DONE]";

/// One decoded SSE payload.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StreamEvent {
    /// Text to append to the response.
    Delta(String),
    /// The stream signalled completion; stop reading.
    Done,
    /// Nothing to append (lifecycle event, keep-alive, unparseable noise).
    Ignore,
}

/// Accumulates raw SSE bytes and yields complete lines.
///
/// Chunk boundaries fall anywhere, so a line can be split across two chunks and
/// has to be held until its newline arrives.
#[derive(Default)]
struct SseBuffer {
    pending: String,
}

impl SseBuffer {
    /// Appends `chunk` and returns every line completed by it.
    ///
    /// Passing `"\n"` flushes whatever is still buffered, which is how a final
    /// line that arrived without a trailing newline gets processed.
    fn push(&mut self, chunk: &str) -> Vec<String> {
        self.pending.push_str(chunk);

        let mut lines = Vec::new();
        while let Some(index) = self.pending.find('\n') {
            lines.push(self.pending[..index].trim().to_string());
            self.pending.drain(..=index);
        }
        lines
    }
}

/// Decodes one SSE `data:` payload.
///
/// Unparseable payloads are [`StreamEvent::Ignore`] on purpose: a single
/// keep-alive, gateway error page or non-standard event must not abort a stream
/// that is otherwise delivering a perfectly good answer.
fn classify_stream_payload(provider: &str, data: &str) -> StreamEvent {
    let data = data.trim();
    if data.is_empty() {
        return StreamEvent::Ignore;
    }
    if data == STREAM_DONE_MARKER {
        return StreamEvent::Done;
    }

    let Ok(json) = serde_json::from_str::<Value>(data) else {
        return StreamEvent::Ignore;
    };

    if is_anthropic_provider(provider) {
        // Anthropic tags every event with a `type`; only content_block_delta
        // carries text.
        if json["type"].as_str() != Some("content_block_delta") {
            return StreamEvent::Ignore;
        }
        match json["delta"]["text"].as_str() {
            Some(text) if !text.is_empty() => StreamEvent::Delta(text.to_string()),
            _ => StreamEvent::Ignore,
        }
    } else {
        match json["choices"][0]["delta"]["content"].as_str() {
            Some(text) if !text.is_empty() => StreamEvent::Delta(text.to_string()),
            _ => StreamEvent::Ignore,
        }
    }
}

/// Applies one SSE line to `content`, returning `true` when the stream is done.
fn apply_stream_line(
    provider: &str,
    line: &str,
    content: &mut String,
    on_chunk: &Channel<PolishStreamEvent>,
) -> Result<bool, String> {
    // Comment lines (`: keep-alive`) and non-`data:` fields (`event:`, `id:`)
    // carry no payload.
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(false);
    };

    match classify_stream_payload(provider, data) {
        StreamEvent::Delta(delta) => {
            content.push_str(&delta);
            emit_polish_delta(on_chunk, delta)?;
            Ok(false)
        }
        StreamEvent::Done => Ok(true),
        StreamEvent::Ignore => Ok(false),
    }
}

async fn chat_completion_stream(
    settings: &ApiSettings,
    system_prompt: &str,
    user_message: &str,
    on_chunk: Channel<PolishStreamEvent>,
) -> Result<String, String> {
    let api_key = settings.api_key.trim();
    if api_key.is_empty() {
        return Err("未找到 API Key，请在设置中重新输入并保存该模型配置".to_string());
    }

    let is_anthropic = is_anthropic_provider(&settings.provider);

    let request_body = if is_anthropic {
        serde_json::json!({
            "model": settings.model,
            "max_tokens": MAX_OUTPUT_TOKENS,
            "system": system_prompt,
            "stream": true,
            "messages": [
                {"role": "user", "content": user_message}
            ]
        })
    } else {
        serde_json::json!({
            "model": settings.model,
            "stream": true,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_message}
            ]
        })
    };

    let endpoint = if is_anthropic {
        format!("{}/messages", settings.base_url.trim_end_matches('/'))
    } else {
        format!(
            "{}/chat/completions",
            settings.base_url.trim_end_matches('/')
        )
    };

    let client = build_client()?;
    let response = apply_auth_headers(
        client
            .post(&endpoint)
            .header("Content-Type", "application/json"),
        &settings.provider,
        api_key,
    )
    .json(&request_body)
    .send()
    .await
    .map_err(|e| {
        if e.is_timeout() {
            "模型请求超时，请检查网络后重试".to_string()
        } else if e.is_connect() {
            "模型请求失败，请检查网络或 API 配置".to_string()
        } else {
            format!("模型请求失败，请检查网络或 API 配置: {}", e)
        }
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API 请求失败 ({status}): {body}"));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = SseBuffer::default();
    let mut content = String::new();
    let mut done = false;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取流式响应失败: {}", e))?;

        for line in buffer.push(&String::from_utf8_lossy(&chunk)) {
            if apply_stream_line(&settings.provider, &line, &mut content, &on_chunk)? {
                done = true;
                break;
            }
        }

        if done {
            break;
        }
    }

    // Flush a final line that arrived without a trailing newline.
    if !done {
        for line in buffer.push("\n") {
            if apply_stream_line(&settings.provider, &line, &mut content, &on_chunk)? {
                break;
            }
        }
    }

    if content.is_empty() {
        Err("模型返回了空内容，请重试".to_string())
    } else {
        on_chunk
            .send(PolishStreamEvent::Done {
                content: content.clone(),
            })
            .map_err(|e| format!("流式结果发送失败: {}", e))?;
        Ok(content)
    }
}


fn extract_json_array(json_like: &str) -> Result<&str, String> {
    let trimmed = json_like.trim();

    if let Some(start) = trimmed.find("```json") {
        let content_start = start + 7;
        let content_end = trimmed[content_start..]
            .find("```")
            .map(|end| content_start + end)
            .unwrap_or(trimmed.len());
        return Ok(trimmed[content_start..content_end].trim());
    }

    if let Some(start) = trimmed.find('[') {
        let end = trimmed
            .rfind(']')
            .ok_or_else(|| "处理要点解析失败：缺少 JSON 数组结束符".to_string())?;
        return Ok(trimmed[start..=end].trim());
    }

    Err("处理要点解析失败：模型未返回 JSON 数组".to_string())
}

fn parse_polish_process_steps(response: &str) -> Result<Vec<PolishProcessStep>, String> {
    let json_str = extract_json_array(response)?;
    let steps: Vec<PolishProcessStep> = serde_json::from_str(json_str)
        .map_err(|e| format!("处理要点解析失败，请重试: {}", e))?;

    let steps: Vec<PolishProcessStep> = steps
        .into_iter()
        .map(|step| PolishProcessStep {
            title: step.title.trim().chars().take(16).collect(),
            desc: step.desc.trim().chars().take(48).collect(),
        })
        .filter(|step| !step.title.is_empty() && !step.desc.is_empty())
        .take(4)
        .collect();

    if steps.is_empty() {
        Err("处理要点解析失败：模型未返回有效步骤".to_string())
    } else {
        Ok(steps)
    }
}


/// AI polish/optimize a prompt — uses the active API config
#[tauri::command]
pub async fn polish_prompt(
    state: tauri::State<'_, DbState>,
    content: String,
    mode: String,
) -> Result<String, String> {
    let settings = api_configs::get_active_settings(&state)?;
    let system_prompt = polish_system_prompt(&mode);

    chat_completion(&settings, system_prompt, &content).await
}

/// Stream AI polish output as chunks and return the final content.
#[tauri::command]
pub async fn polish_prompt_stream(
    state: tauri::State<'_, DbState>,
    content: String,
    mode: String,
    on_chunk: Channel<PolishStreamEvent>,
) -> Result<String, String> {
    let settings = api_configs::get_active_settings(&state)?;
    let system_prompt = polish_system_prompt(&mode);

    chat_completion_stream(&settings, system_prompt, &content, on_chunk).await
}

/// Generate user-visible AI process steps for the selected polish mode.
#[tauri::command]
pub async fn analyze_polish_process(
    state: tauri::State<'_, DbState>,
    content: String,
    mode: String,
) -> Result<Vec<PolishProcessStep>, String> {
    let settings = api_configs::get_active_settings(&state)?;
    let user_message = format!(
        "润色模式：{}\n用户原始 Prompt：\n{}",
        polish_mode_label(&mode),
        content
    );

    let response = chat_completion(&settings, POLISH_PROCESS_PROMPT, &user_message).await?;
    parse_polish_process_steps(&response)
}

/// Auto-classify a prompt — uses the active API config
#[tauri::command]
pub async fn auto_classify(
    state: tauri::State<'_, DbState>,
    content: String,
) -> Result<ClassifyResult, String> {
    let settings = api_configs::get_active_settings(&state)?;

    let system_prompt = CLASSIFY_SYSTEM_PROMPT;

    let response = chat_completion(&settings, system_prompt, &content).await?;
    let json_str = response.trim();

    let json_str = if let Some(start) = json_str.find("```json") {
        let start = start + 7;
        let end = json_str[start..]
            .find("```")
            .unwrap_or(json_str.len() - start);
        &json_str[start..start + end]
    } else if let Some(start) = json_str.find('{') {
        let end = json_str.rfind('}').unwrap_or(json_str.len() - 1);
        &json_str[start..=end]
    } else {
        json_str
    };

    serde_json::from_str::<ClassifyResult>(json_str)
        .map_err(|e| format!("自动分类解析失败，请重试: {}", e))
}

/// Test connection for a specific API config with the provided API key.
#[tauri::command]
pub async fn test_connection(
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
) -> Result<String, String> {
    let config_id = Uuid::new_v4().to_string();
    let settings = ApiSettings {
        config_id: config_id.clone(),
        provider,
        base_url,
        api_key,
        model,
    };

    let test_prompt = "请回复：连接成功";
    let result = chat_completion(&settings, "你是一个测试助手", test_prompt).await;

    match result {
        Ok(_) => Ok("连接成功".to_string()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn parse_polish_process_steps_accepts_json_array_with_wrapping_text() {
        let response = r#"说明：
[
  { "title": "识别任务", "desc": "围绕每日 AI 新闻推送定位目标" },
  { "title": "补充边界", "desc": "明确质量标准和推送形式" }
]
"#;

        let steps = parse_polish_process_steps(response).unwrap();

        assert_eq!(
            steps,
            vec![
                PolishProcessStep {
                    title: "识别任务".to_string(),
                    desc: "围绕每日 AI 新闻推送定位目标".to_string(),
                },
                PolishProcessStep {
                    title: "补充边界".to_string(),
                    desc: "明确质量标准和推送形式".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_polish_process_steps_rejects_non_json_output() {
        let error = parse_polish_process_steps("我会先分析，再优化").unwrap_err();

        assert!(error.contains("JSON 数组"));
    }

    // ------------------ Request auth headers ------------------

    fn built_headers(provider: &str) -> reqwest::header::HeaderMap {
        apply_auth_headers(
            Client::new().post("https://example.com/v1/messages"),
            provider,
            "secret-key",
        )
        .build()
        .expect("build request")
        .headers()
        .clone()
    }

    #[test]
    fn anthropic_requests_declare_the_required_version_header() {
        // 回归测试：缺少 anthropic-version 时 Messages API 直接返回
        // 400 "anthropic-version: header is required"，Anthropic 配置完全不可用。
        let headers = built_headers("anthropic");

        assert_eq!(headers["anthropic-version"], ANTHROPIC_API_VERSION);
        assert_eq!(headers["x-api-key"], "secret-key");
        assert!(
            headers.get("authorization").is_none(),
            "Anthropic 不用 Bearer 认证"
        );
    }

    #[test]
    fn openai_compatible_requests_use_bearer_auth_only() {
        let headers = built_headers("deepseek");

        assert_eq!(headers["authorization"], "Bearer secret-key");
        assert!(
            headers.get("anthropic-version").is_none(),
            "非 Anthropic provider 不应带 anthropic-version"
        );
        assert!(headers.get("x-api-key").is_none());
    }

    #[test]
    fn provider_matching_is_case_insensitive_and_substring_based() {
        assert!(is_anthropic_provider("Anthropic"));
        assert!(is_anthropic_provider("ANTHROPIC"));
        assert!(!is_anthropic_provider("openai"));
        assert!(!is_anthropic_provider("kimi"));
    }

    // ------------------ Timeouts ------------------

    #[test]
    fn build_client_succeeds_with_configured_timeouts() {
        // 超时常量本身由模块顶部的 const 断言在编译期守护。
        assert!(build_client().is_ok());
    }

    #[test]
    fn http_client_is_created_once_and_shared() {
        // 回归测试：以前每个请求都新建 Client，连接池无法复用。
        let _ = build_client().expect("build client");

        assert!(
            HTTP_CLIENT.get().is_some(),
            "Client 应被缓存并复用，而不是每次请求重新创建"
        );
    }

    // ------------------ SSE buffering ------------------

    #[test]
    fn sse_buffer_splits_lines_that_span_chunk_boundaries() {
        let mut buffer = SseBuffer::default();

        assert!(buffer.push("data: {\"a\":").is_empty(), "未收满一行不应产出");
        assert_eq!(
            buffer.push("1}\ndata: {\"b\":2}\n"),
            vec!["data: {\"a\":1}", "data: {\"b\":2}"]
        );
    }

    #[test]
    fn sse_buffer_ignores_carriage_returns_and_blank_lines() {
        let mut buffer = SseBuffer::default();

        assert_eq!(
            buffer.push("data: x\r\n\r\ndata: y\r\n"),
            vec!["data: x", "", "data: y"]
        );
    }

    #[test]
    fn sse_buffer_flushes_a_trailing_line_without_a_newline() {
        // 回归测试：流结束时最后一行常常没有换行符，旧实现直接把它丢了 ——
        // 表现为回答的最后一个分片凭空消失。
        let mut buffer = SseBuffer::default();

        assert!(buffer.push("data: {\"tail\":true}").is_empty());

        assert_eq!(buffer.push("\n"), vec!["data: {\"tail\":true}"]);
    }

    // ------------------ Stream payload classification ------------------

    fn classify(provider: &str, data: &str) -> StreamEvent {
        classify_stream_payload(provider, data)
    }

    #[test]
    fn openai_stream_deltas_are_extracted() {
        let payload = r#"{"choices":[{"delta":{"content":"你好"}}]}"#;

        assert_eq!(
            classify("deepseek", payload),
            StreamEvent::Delta("你好".to_string())
        );
    }

    #[test]
    fn openai_role_only_delta_is_ignored() {
        // 首个分片只有 role 没有 content，不应产生空 delta。
        let payload = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;

        assert_eq!(classify("deepseek", payload), StreamEvent::Ignore);
    }

    #[test]
    fn anthropic_content_block_delta_is_extracted() {
        let payload = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"世界"}}"#;

        assert_eq!(
            classify("anthropic", payload),
            StreamEvent::Delta("世界".to_string())
        );
    }

    #[test]
    fn anthropic_lifecycle_events_are_ignored() {
        for payload in [
            r#"{"type":"message_start","message":{"id":"m"}}"#,
            r#"{"type":"content_block_start","index":0}"#,
            r#"{"type":"content_block_stop","index":0}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#,
            r#"{"type":"message_stop"}"#,
        ] {
            assert_eq!(
                classify("anthropic", payload),
                StreamEvent::Ignore,
                "应忽略生命周期事件: {payload}"
            );
        }
    }

    #[test]
    fn done_marker_is_recognised() {
        assert_eq!(classify("deepseek", "[DONE]"), StreamEvent::Done);
    }

    #[test]
    fn unparseable_payloads_are_skipped_not_fatal() {
        // 回归测试：旧实现在遇到第一个非 JSON 行时用 `?` 中断整个流。
        // 网关噪声、部分服务的注释行、心跳都不该毁掉一次正常的回答。
        for payload in ["", "  ", "<html>502 Bad Gateway</html>", "ping", "{not json}"] {
            assert_eq!(
                classify("deepseek", payload),
                StreamEvent::Ignore,
                "应跳过而非报错: {payload:?}"
            );
        }
    }

    #[test]
    fn anthropic_error_events_surface_no_delta() {
        // Anthropic 在流中途可能推送 error 事件；不解析出 delta，但也不 panic。
        let payload = r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#;

        assert_eq!(classify("anthropic", payload), StreamEvent::Ignore);
    }
}

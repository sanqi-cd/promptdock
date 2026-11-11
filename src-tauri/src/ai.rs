use futures_util::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use uuid::Uuid;

use crate::api_configs;
use crate::commands::DbState;
use crate::polish_prompts::{
    classify_system_prompt, polish_mode_label, polish_system_prompt, POLISH_PROCESS_PROMPT,
};

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

/// Error string returned when the user cancels an in-flight polish.
///
/// Kept ASCII so the frontend can match it exactly; it never reaches the UI
/// (the store converts it to an empty result).
pub(crate) const POLISH_CANCELLED: &str = "POLISH_CANCELLED";

/// Cooperative cancellation for one in-flight polish request.
///
/// An `AtomicBool` alone is only observable between chunks, so cancelling
/// while the server is silent still waits for the 120s idle-read timeout.
/// `notify_one` wakes a pending `send()` / `stream.next()` immediately.
#[derive(Default)]
struct CancelToken {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}

impl CancelToken {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // `notify_one` stores a permit when no waiter is registered, so a
        // cancel that lands between the flag check and `notified()` is not lost.
        self.notify.notify_one();
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Resolves once cancellation is requested. Cancel-safe in `tokio::select!`.
    async fn wait_cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

/// Cancellation tokens for in-flight streaming polish requests, keyed by the
/// `request_id` the frontend passes to `polish_prompt_stream`.
#[derive(Default)]
pub struct PolishCancelState(Mutex<HashMap<String, Arc<CancelToken>>>);

impl PolishCancelState {
    fn register(&self, request_id: &str) -> Result<Arc<CancelToken>, String> {
        let mut tokens = self.0.lock().map_err(|e| e.to_string())?;
        if tokens.contains_key(request_id) {
            return Err(format!("流式请求 ID 重复: {request_id}"));
        }

        let token = Arc::new(CancelToken::default());
        tokens.insert(request_id.to_string(), token.clone());
        Ok(token)
    }

    /// Signals cancellation if the request is still in flight. Unknown ids are
    /// a no-op, which keeps cancellation idempotent.
    fn cancel(&self, request_id: &str) -> Result<(), String> {
        if let Some(token) = self.0.lock().map_err(|e| e.to_string())?.get(request_id) {
            token.cancel();
        }
        Ok(())
    }

    /// Removes only the registration that owns `token`. A stale guard must not
    /// accidentally remove a newer request that happens to reuse the same id.
    fn unregister(&self, request_id: &str, token: &Arc<CancelToken>) -> Result<(), String> {
        let mut tokens = self.0.lock().map_err(|e| e.to_string())?;
        let owns_registration = tokens
            .get(request_id)
            .is_some_and(|current| Arc::ptr_eq(current, token));
        if owns_registration {
            tokens.remove(request_id);
        }
        Ok(())
    }
}

/// RAII registration: command errors and dropped command futures both clean up
/// their token, without relying on an explicit unregister after `.await`.
struct CancelRegistration<'a> {
    state: &'a PolishCancelState,
    request_id: String,
    token: Arc<CancelToken>,
}

impl<'a> CancelRegistration<'a> {
    fn new(state: &'a PolishCancelState, request_id: String) -> Result<Self, String> {
        let token = state.register(&request_id)?;
        Ok(Self {
            state,
            request_id,
            token,
        })
    }
}

impl Drop for CancelRegistration<'_> {
    fn drop(&mut self) {
        let _ = self.state.unregister(&self.request_id, &self.token);
    }
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

/// 流式请求专用客户端：不设置覆盖整个响应体的总超时。
///
/// reqwest 的 `timeout` 是「从开始连接直到响应体读完」，会把超过 300 秒的长流
/// 从中间掐断。流式请求只有 connect + read(idle) 超时，只要模型持续输出就不会
/// 被误杀；连接静默 120 秒仍会被 read_timeout 终止。
static STREAM_HTTP_CLIENT: std::sync::OnceLock<Result<Client, String>> = std::sync::OnceLock::new();

/// 返回共享客户端（首次调用时创建）。
fn build_client() -> Result<Client, String> {
    HTTP_CLIENT.get_or_init(create_client).clone()
}

/// 返回流式请求专用客户端（首次调用时创建）。
fn build_stream_client() -> Result<Client, String> {
    STREAM_HTTP_CLIENT.get_or_init(create_stream_client).clone()
}

fn configured_client_builder() -> reqwest::ClientBuilder {
    Client::builder()
        .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .read_timeout(std::time::Duration::from_secs(READ_TIMEOUT_SECS))
        // 不跟随重定向。reqwest 跨主机重定向时只剥离 Authorization / Cookie，
        // 自定义的 x-api-key（Anthropic）会被原样转发到重定向目标。
        .redirect(reqwest::redirect::Policy::none())
}

fn create_client() -> Result<Client, String> {
    configured_client_builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

fn create_stream_client() -> Result<Client, String> {
    configured_client_builder()
        .build()
        .map_err(|e| format!("Failed to create streaming HTTP client: {}", e))
}

/// Caps how much of an upstream error body is echoed back to the webview.
/// Without a cap, an arbitrary endpoint (including a local service reached via
/// `test_connection`) could stream a large response into the UI.
const MAX_ERROR_BODY_CHARS: usize = 500;
const MAX_ERROR_BODY_BYTES: usize = 16 * 1024;
const MAX_JSON_BODY_BYTES: usize = 8 * 1024 * 1024;

fn truncate_error_body(body: &str) -> String {
    let mut chars = body.chars();
    let prefix: String = chars.by_ref().take(MAX_ERROR_BODY_CHARS).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

/// Reads at most `max_bytes + 1` bytes. The extra byte lets successful JSON
/// callers distinguish an oversized body from one that exactly fits the cap.
/// Every read can race cancellation, so a slow drip cannot hold a cancelled
/// streaming request open indefinitely.
async fn read_body_with_limit<S, C, SE>(
    mut stream: S,
    max_bytes: usize,
    cancelled: Option<&CancelToken>,
) -> Result<Vec<u8>, String>
where
    S: Stream<Item = Result<C, SE>> + Unpin,
    C: AsRef<[u8]>,
    SE: std::fmt::Display,
{
    let mut body = Vec::new();
    loop {
        let next = if let Some(cancelled) = cancelled {
            tokio::select! {
                biased;
                _ = cancelled.wait_cancelled() => {
                    return Err(POLISH_CANCELLED.to_string());
                }
                next = stream.next() => next,
            }
        } else {
            stream.next().await
        };

        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(|e| format!("读取响应体失败: {}", e))?;
        let bytes = chunk.as_ref();
        if bytes.is_empty() {
            continue;
        }

        let remaining = max_bytes + 1 - body.len();
        let take = remaining.min(bytes.len());
        body.extend_from_slice(&bytes[..take]);
        if body.len() > max_bytes {
            break;
        }
    }
    Ok(body)
}

async fn read_error_response_body(
    response: reqwest::Response,
    cancelled: Option<&CancelToken>,
) -> Result<String, String> {
    let mut bytes =
        read_body_with_limit(response.bytes_stream(), MAX_ERROR_BODY_BYTES, cancelled).await?;
    bytes.truncate(MAX_ERROR_BODY_BYTES);
    Ok(truncate_error_body(&String::from_utf8_lossy(&bytes)))
}

async fn read_json_response(response: reqwest::Response) -> Result<Value, String> {
    let bytes = read_body_with_limit(response.bytes_stream(), MAX_JSON_BODY_BYTES, None).await?;
    if bytes.len() > MAX_JSON_BODY_BYTES {
        return Err("模型响应过大，已中止解析".to_string());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
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
        let body = read_error_response_body(response, None).await?;
        return Err(format!("API 请求失败 ({status}): {body}"));
    }

    let json = read_json_response(response).await?;

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

fn emit_polish_delta(on_chunk: &Channel<PolishStreamEvent>, delta: String) -> Result<(), String> {
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
    /// OpenAI can put the final text and `finish_reason` in the same chunk.
    DeltaAndDone(String),
    /// The provider explicitly signalled completion; stop reading.
    Done,
    /// An upstream error is fatal even if earlier deltas were already emitted.
    Error(String),
    /// Nothing to append (lifecycle event, keep-alive, unparseable noise).
    Ignore,
}

/// Accumulates raw SSE bytes and yields complete lines.
///
/// Chunk boundaries fall anywhere, so a line can be split across two chunks and
/// has to be held until its newline arrives. Buffering **bytes** — not decoded
/// text — is what keeps multi-byte UTF-8 characters (e.g. 3-byte Chinese)
/// intact when they straddle a chunk boundary: decoding happens only on
/// complete lines, and a line boundary never splits a character.
/// Upper bound for a single un-terminated SSE line. A malicious or broken
/// endpoint (base_url is user-controlled) must not be able to grow the buffer
/// without bound by never sending a newline.
const SSE_LINE_MAX_BYTES: usize = 1024 * 1024;

#[derive(Default)]
struct SseBuffer {
    pending: Vec<u8>,
}

impl SseBuffer {
    /// Appends `chunk` and returns every line completed by it.
    ///
    /// Passing `b"\n"` flushes whatever is still buffered, which is how a final
    /// line that arrived without a trailing newline gets processed.
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        self.pending.extend_from_slice(chunk);

        let mut lines = Vec::new();
        while let Some(index) = self.pending.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=index).collect();
            line.pop(); // drop the '\n'
            lines.push(decode_sse_line(&line));
        }

        if self.pending.len() > SSE_LINE_MAX_BYTES {
            return Err("流式响应单行过长，已中止".to_string());
        }

        Ok(lines)
    }
}

/// Decodes one complete SSE line. The strict path should always succeed — a
/// line boundary cannot split a multi-byte character — so lossy decoding is
/// only a safety net for a peer that sends genuinely invalid bytes.
fn decode_sse_line(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.trim().to_string(),
        Err(_) => String::from_utf8_lossy(bytes).trim().to_string(),
    }
}

/// Collects the `data:` lines of one SSE event. A blank line dispatches the
/// event; multiple data lines are joined with newlines as required by SSE.
#[derive(Default)]
struct SseEventBuffer {
    data_lines: Vec<String>,
    data_bytes: usize,
}

impl SseEventBuffer {
    fn push_line(&mut self, line: &str) -> Result<Option<String>, String> {
        if line.is_empty() {
            return Ok(self.finish());
        }
        let data = if let Some(data) = line.strip_prefix("data:") {
            data.strip_prefix(' ').unwrap_or(data)
        } else if line == "data" {
            ""
        } else {
            return Ok(None);
        };

        let additional = data.len() + usize::from(!self.data_lines.is_empty());
        if self.data_bytes.saturating_add(additional) > SSE_LINE_MAX_BYTES {
            return Err("流式响应单个事件过长，已中止".to_string());
        }
        self.data_bytes += additional;
        self.data_lines.push(data.to_string());
        Ok(None)
    }

    fn finish(&mut self) -> Option<String> {
        self.data_bytes = 0;
        if self.data_lines.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.data_lines).join("\n"))
        }
    }
}

fn upstream_stream_error(json: &Value) -> String {
    let error = json.get("error").filter(|error| !error.is_null());
    let message = error
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| error.and_then(Value::as_str).map(str::to_string))
        .or_else(|| error.map(Value::to_string))
        .or_else(|| {
            json.get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "未知错误".to_string());
    format!("上游模型返回错误: {message}")
}

/// Decodes one aggregated SSE `data:` payload.
///
/// Unparseable payloads are [`StreamEvent::Ignore`] on purpose: a single
/// keep-alive, gateway error page or non-standard event must not abort a stream
/// that is otherwise delivering a perfectly good answer. Explicit upstream
/// errors are different: they must fail the request, never complete it with a
/// previously emitted prefix.
fn classify_stream_payload(provider: &str, data: &str) -> StreamEvent {
    let data = data.trim();
    if data.is_empty() {
        return StreamEvent::Ignore;
    }
    let is_anthropic = is_anthropic_provider(provider);
    if !is_anthropic && data == STREAM_DONE_MARKER {
        return StreamEvent::Done;
    }

    let Ok(json) = serde_json::from_str::<Value>(data) else {
        return StreamEvent::Ignore;
    };

    if (is_anthropic && json["type"].as_str() == Some("error"))
        || json.get("error").is_some_and(|error| !error.is_null())
    {
        return StreamEvent::Error(upstream_stream_error(&json));
    }

    if is_anthropic {
        match json["type"].as_str() {
            Some("message_stop") => StreamEvent::Done,
            Some("message_delta") => match json["delta"]["stop_reason"].as_str() {
                Some("max_tokens") => StreamEvent::Error(
                    "模型输出因达到长度上限而被截断，请调大输出上限后重试".to_string(),
                ),
                Some("refusal") => {
                    StreamEvent::Error("模型拒绝生成内容，请调整提示词后重试".to_string())
                }
                _ => StreamEvent::Ignore,
            },
            Some("content_block_delta") => match json["delta"]["text"].as_str() {
                Some(text) if !text.is_empty() => StreamEvent::Delta(text.to_string()),
                _ => StreamEvent::Ignore,
            },
            _ => StreamEvent::Ignore,
        }
    } else {
        let choice = &json["choices"][0];
        let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
        match finish_reason {
            Some("length") => {
                return StreamEvent::Error(
                    "模型输出因达到长度上限而被截断，请调大输出上限后重试".to_string(),
                );
            }
            Some("content_filter") => {
                return StreamEvent::Error(
                    "模型输出被内容安全策略过滤，请调整提示词后重试".to_string(),
                );
            }
            _ => {}
        }
        let done = choice
            .get("finish_reason")
            .is_some_and(|reason| !reason.is_null());
        let delta = choice["delta"]["content"]
            .as_str()
            .filter(|text| !text.is_empty());
        match (delta, done) {
            (Some(text), true) => StreamEvent::DeltaAndDone(text.to_string()),
            (Some(text), false) => StreamEvent::Delta(text.to_string()),
            (None, true) => StreamEvent::Done,
            (None, false) => StreamEvent::Ignore,
        }
    }
}

/// Applies one aggregated SSE event, returning `true` only for an explicit
/// provider completion signal.
fn apply_stream_event<E>(
    provider: &str,
    data: &str,
    content: &mut String,
    emit: &mut E,
) -> Result<bool, String>
where
    E: FnMut(String) -> Result<(), String>,
{
    match classify_stream_payload(provider, data) {
        StreamEvent::Delta(delta) => {
            content.push_str(&delta);
            emit(delta)?;
            Ok(false)
        }
        StreamEvent::DeltaAndDone(delta) => {
            content.push_str(&delta);
            emit(delta)?;
            Ok(true)
        }
        StreamEvent::Done => Ok(true),
        StreamEvent::Error(error) => Err(error),
        StreamEvent::Ignore => Ok(false),
    }
}

fn apply_stream_line<E>(
    provider: &str,
    line: &str,
    events: &mut SseEventBuffer,
    content: &mut String,
    emit: &mut E,
) -> Result<bool, String>
where
    E: FnMut(String) -> Result<(), String>,
{
    // Comments and non-data fields (`event:`, `id:`, `retry:`) do not dispatch
    // an event. Only an empty line does.
    let Some(data) = events.push_line(line)? else {
        return Ok(false);
    };
    apply_stream_event(provider, &data, content, emit)
}

/// Reads SSE byte chunks to an explicit provider completion signal, forwarding
/// each delta through `emit` and returning the accumulated content. EOF without
/// `[DONE]`, `finish_reason` or `message_stop` is an error, not a successful
/// prefix. Cancellation races every read so a silent upstream cannot keep the
/// request alive until the read timeout.
async fn consume_sse_stream<S, C, SE, E>(
    provider: &str,
    mut stream: S,
    emit: &mut E,
    cancelled: &CancelToken,
) -> Result<String, String>
where
    S: Stream<Item = Result<C, SE>> + Unpin,
    C: AsRef<[u8]>,
    SE: std::fmt::Display,
    E: FnMut(String) -> Result<(), String>,
{
    let mut buffer = SseBuffer::default();
    let mut events = SseEventBuffer::default();
    let mut content = String::new();
    let mut done = false;

    loop {
        let next = tokio::select! {
            biased;
            _ = cancelled.wait_cancelled() => {
                return Err(POLISH_CANCELLED.to_string());
            }
            next = stream.next() => next,
        };

        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(|e| format!("读取流式响应失败: {}", e))?;

        for line in buffer.push(chunk.as_ref())? {
            if apply_stream_line(provider, &line, &mut events, &mut content, emit)? {
                done = true;
                break;
            }
        }

        if done {
            break;
        }
    }

    // Flush a final line that arrived without a trailing newline, then dispatch
    // a final event even if the peer omitted its terminating blank line.
    if !done {
        for line in buffer.push(b"\n")? {
            if apply_stream_line(provider, &line, &mut events, &mut content, emit)? {
                done = true;
                break;
            }
        }
    }
    if !done {
        if let Some(data) = events.finish() {
            done = apply_stream_event(provider, &data, &mut content, emit)?;
        }
    }

    // A cancel that lands during the flush still wins: callers must not treat
    // truncated content as a complete result.
    if cancelled.is_cancelled() {
        return Err(POLISH_CANCELLED.to_string());
    }
    if !done {
        return Err("流式响应提前结束，未收到正常结束标记".to_string());
    }

    Ok(content)
}

async fn chat_completion_stream(
    settings: &ApiSettings,
    system_prompt: &str,
    user_message: &str,
    on_chunk: Channel<PolishStreamEvent>,
    cancelled: &CancelToken,
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

    let client = build_stream_client()?;
    let request = apply_auth_headers(
        client
            .post(&endpoint)
            .header("Content-Type", "application/json"),
        &settings.provider,
        api_key,
    )
    .json(&request_body);

    // Race the request itself: cancelling before the response headers arrive
    // should not wait for connect/read timeouts (15s / 120s).
    let response = tokio::select! {
        biased;
        _ = cancelled.wait_cancelled() => {
            return Err(POLISH_CANCELLED.to_string());
        }
        response = request.send() => response,
    }
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
        let body = read_error_response_body(response, Some(cancelled)).await?;
        return Err(format!("API 请求失败 ({status}): {body}"));
    }

    let stream = response.bytes_stream();
    let mut emit = |delta: String| emit_polish_delta(&on_chunk, delta);
    let content = consume_sse_stream(&settings.provider, stream, &mut emit, cancelled).await?;

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
        // `]` 可能出现在 `[` 之前（如模型输出 `"]["`），此时 end < start，
        // 直接切片会 panic —— 走正常错误路径拒绝。
        if end < start {
            return Err("处理要点解析失败：JSON 数组括号顺序异常".to_string());
        }
        return Ok(trimmed[start..=end].trim());
    }

    Err("处理要点解析失败：模型未返回 JSON 数组".to_string())
}

fn parse_polish_process_steps(response: &str) -> Result<Vec<PolishProcessStep>, String> {
    let json_str = extract_json_array(response)?;
    let steps: Vec<PolishProcessStep> =
        serde_json::from_str(json_str).map_err(|e| format!("处理要点解析失败，请重试: {}", e))?;

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
///
/// `request_id` identifies this invocation so `cancel_polish_stream` can abort
/// it mid-flight; the frontend generates one per call.
#[tauri::command]
pub async fn polish_prompt_stream(
    state: tauri::State<'_, DbState>,
    cancel_state: tauri::State<'_, PolishCancelState>,
    request_id: String,
    content: String,
    mode: String,
    on_chunk: Channel<PolishStreamEvent>,
) -> Result<String, String> {
    // Register before reading configuration so even an extremely early cancel
    // can observe this request. The guard also cleans up if this future is
    // dropped while awaiting configuration or the network.
    let registration = CancelRegistration::new(cancel_state.inner(), request_id)?;
    let settings = api_configs::get_active_settings(&state)?;
    let system_prompt = polish_system_prompt(&mode);

    chat_completion_stream(
        &settings,
        system_prompt,
        &content,
        on_chunk,
        &registration.token,
    )
    .await
}

/// Cancel an in-flight `polish_prompt_stream`. Idempotent: unknown or already
/// finished request ids are silently ignored.
#[tauri::command]
pub fn cancel_polish_stream(
    cancel_state: tauri::State<'_, PolishCancelState>,
    request_id: String,
) -> Result<(), String> {
    cancel_state.cancel(&request_id)
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

/// Keep category input bounded and single-line so custom names cannot distort
/// the system prompt structure. Empty input falls back to the built-in list.
fn normalize_classify_categories(categories: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for raw in categories {
        let single_line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if single_line.is_empty() {
            continue;
        }
        let name: String = single_line.chars().take(30).collect();
        if !normalized.contains(&name) {
            normalized.push(name);
        }
        if normalized.len() == 30 {
            break;
        }
    }
    normalized
}

/// Auto-classify a prompt — uses the active API config and the user's current categories.
#[tauri::command]
pub async fn auto_classify(
    state: tauri::State<'_, DbState>,
    content: String,
    categories: Option<Vec<String>>,
) -> Result<ClassifyResult, String> {
    let settings = api_configs::get_active_settings(&state)?;
    let categories = normalize_classify_categories(categories.unwrap_or_default());
    let system_prompt = classify_system_prompt(&categories);

    let response = chat_completion(&settings, &system_prompt, &content).await?;
    let json_str = extract_json_object(response.trim())?;

    serde_json::from_str::<ClassifyResult>(json_str)
        .map_err(|e| format!("自动分类解析失败，请重试: {}", e))
}

/// Extracts the JSON object from a model response that may wrap it in a
/// ```json fence or surrounding prose.
fn extract_json_object(response: &str) -> Result<&str, String> {
    if let Some(start) = response.find("```json") {
        let start = start + 7;
        let end = response[start..]
            .find("```")
            .unwrap_or(response.len() - start);
        return Ok(response[start..start + end].trim());
    }

    if let Some(start) = response.find('{') {
        let end = response.rfind('}').unwrap_or(response.len() - 1);
        // `}` 可能出现在 `{` 之前（如模型输出 `"}{"`），此时 end < start，
        // 直接切片会 panic —— 走正常错误路径拒绝。
        if end < start {
            return Err("自动分类解析失败：JSON 对象括号顺序异常".to_string());
        }
        return Ok(&response[start..=end]);
    }

    Ok(response)
}

/// Test connection for a specific API config with the provided API key.
#[tauri::command]
pub async fn test_connection(
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
) -> Result<String, String> {
    // `test_connection` is callable directly over IPC and receives its own URL,
    // so it has to enforce the same rules as saved configs — otherwise it
    // becomes an unvalidated SSRF primitive for local services.
    crate::api_configs::validate_base_url(base_url.trim())?;
    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".to_string());
    }
    if model.trim().is_empty() {
        return Err("模型名称不能为空".to_string());
    }

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

    #[test]
    fn classify_categories_are_single_line_bounded_and_deduplicated() {
        let categories = normalize_classify_categories(vec![
            "  内容创作  ".to_string(),
            "内容创作".to_string(),
            "私人\n知识库".to_string(),
            "   ".to_string(),
        ]);
        assert_eq!(
            categories,
            vec!["内容创作".to_string(), "私人 知识库".to_string()]
        );
    }

    // ------------------ Timeouts ------------------

    #[test]
    fn build_client_succeeds_with_configured_timeouts() {
        // 超时常量本身由模块顶部的 const 断言在编译期守护。
        assert!(build_client().is_ok());
        assert!(build_stream_client().is_ok());
    }

    #[test]
    fn upstream_error_bodies_are_truncated() {
        let short = "invalid key";
        assert_eq!(truncate_error_body(short), short);

        let long = "错".repeat(MAX_ERROR_BODY_CHARS + 1);
        let truncated = truncate_error_body(&long);
        assert_eq!(truncated.chars().count(), MAX_ERROR_BODY_CHARS + 1);
        assert!(truncated.ends_with('…'));
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

        assert!(
            buffer.push(b"data: {\"a\":").expect("push").is_empty(),
            "未收满一行不应产出"
        );
        assert_eq!(
            buffer.push(b"1}\ndata: {\"b\":2}\n").expect("push"),
            vec!["data: {\"a\":1}", "data: {\"b\":2}"]
        );
    }

    #[test]
    fn sse_buffer_ignores_carriage_returns_and_blank_lines() {
        let mut buffer = SseBuffer::default();

        assert_eq!(
            buffer.push(b"data: x\r\n\r\ndata: y\r\n").expect("push"),
            vec!["data: x", "", "data: y"]
        );
    }

    #[test]
    fn sse_buffer_flushes_a_trailing_line_without_a_newline() {
        // 回归测试：流结束时最后一行常常没有换行符，旧实现直接把它丢了 ——
        // 表现为回答的最后一个分片凭空消失。
        let mut buffer = SseBuffer::default();

        assert!(buffer
            .push(b"data: {\"tail\":true}")
            .expect("push")
            .is_empty());

        assert_eq!(
            buffer.push(b"\n").expect("flush"),
            vec!["data: {\"tail\":true}"]
        );
    }

    #[test]
    fn sse_buffer_rejects_an_unterminated_line_that_grows_too_large() {
        let mut buffer = SseBuffer::default();
        let chunk = vec![b'a'; SSE_LINE_MAX_BYTES + 1];

        assert!(
            buffer.push(&chunk).is_err(),
            "没有换行且超过上限时应中止，而不是无限增长内存"
        );
    }

    #[test]
    fn sse_buffer_decodes_multibyte_chars_split_across_chunks() {
        // 回归测试：旧实现按 chunk 做 from_utf8_lossy，中文字符（3 字节）恰好
        // 跨 chunk 时跨界字节变成 U+FFFD，润色结果混入乱码。
        let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"你好，世界\"}}]}\n";
        let bytes = payload.as_bytes();
        // 断点落在「你」的 3 个字节中间。
        let split = bytes
            .windows(3)
            .position(|w| w == "你".as_bytes())
            .expect("payload 应包含中文")
            + 1;

        let mut buffer = SseBuffer::default();
        assert!(buffer.push(&bytes[..split]).expect("push").is_empty());
        let lines = buffer.push(&bytes[split..]).expect("push");

        assert_eq!(lines.len(), 1);
        assert!(!lines[0].contains('\u{FFFD}'), "不应出现替换字符");
        let data = lines[0].strip_prefix("data:").expect("data 行");
        assert_eq!(
            classify_stream_payload("deepseek", data),
            StreamEvent::Delta("你好，世界".to_string())
        );
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
        for payload in [
            "",
            "  ",
            "<html>502 Bad Gateway</html>",
            "ping",
            "{not json}",
        ] {
            assert_eq!(
                classify("deepseek", payload),
                StreamEvent::Ignore,
                "应跳过而非报错: {payload:?}"
            );
        }
    }

    #[test]
    fn anthropic_error_events_are_fatal() {
        let payload = r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#;

        let StreamEvent::Error(error) = classify("anthropic", payload) else {
            panic!("Anthropic error 事件必须使请求失败");
        };
        assert!(error.contains("busy"));
    }

    #[test]
    fn openai_error_payloads_are_fatal() {
        let payload = r#"{"error":{"type":"server_error","message":"broken upstream"}}"#;

        let StreamEvent::Error(error) = classify("deepseek", payload) else {
            panic!("OpenAI error payload 必须使请求失败");
        };
        assert!(error.contains("broken upstream"));
    }

    #[test]
    fn provider_completion_signals_are_recognised() {
        assert_eq!(
            classify("anthropic", r#"{"type":"message_stop"}"#),
            StreamEvent::Done
        );
        assert_eq!(
            classify(
                "deepseek",
                r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            ),
            StreamEvent::Done
        );
        for reason in ["tool_calls", "function_call"] {
            let payload = format!(r#"{{"choices":[{{"delta":{{}},"finish_reason":"{reason}"}}]}}"#);
            assert_eq!(classify("deepseek", &payload), StreamEvent::Done);
        }
        assert_eq!(
            classify(
                "deepseek",
                r#"{"choices":[{"delta":{"content":"tail"},"finish_reason":"stop"}]}"#,
            ),
            StreamEvent::DeltaAndDone("tail".to_string())
        );
    }

    // ------------------ Malformed LLM JSON must not panic ------------------

    #[test]
    fn extract_json_array_rejects_reversed_brackets() {
        // 回归测试：`"]["` 之类输出会让 `]` 出现在 `[` 之前，旧实现的
        // `trimmed[start..=end]` 切片直接 panic。
        for input in ["][", "]x[", "] ["] {
            assert!(
                extract_json_array(input).is_err(),
                "应返回 Err 而非 panic: {input:?}"
            );
        }
    }

    #[test]
    fn extract_json_object_rejects_reversed_braces() {
        // 回归测试：`"}{"` 之类输出会让 `}` 出现在 `{` 之前，旧实现同款 panic。
        for input in ["}{", "}x{", "} {"] {
            assert!(
                extract_json_object(input).is_err(),
                "应返回 Err 而非 panic: {input:?}"
            );
        }
    }

    #[test]
    fn extract_json_object_still_accepts_fence_and_plain_object() {
        let fenced = "```json\n{\"a\":1}\n```";
        assert_eq!(extract_json_object(fenced).unwrap(), "{\"a\":1}");
        assert_eq!(
            extract_json_object("前言 {\"a\":1} 后记").unwrap(),
            "{\"a\":1}"
        );
        // 没有任何括号时原样返回，交给 serde 报错。
        assert_eq!(extract_json_object("不是 JSON").unwrap(), "不是 JSON");
    }

    // ------------------ Stream cancellation ------------------

    #[test]
    fn cancel_unknown_request_id_is_a_noop() {
        // 取消必须幂等：前端可能因时序取消一个已结束或从未存在的请求。
        let state = PolishCancelState::default();

        state
            .cancel("nonexistent")
            .expect("取消不存在的 id 不应报错");
    }

    #[test]
    fn cancel_sets_the_flag_and_unregister_cleans_up() {
        let state = PolishCancelState::default();

        let token = state.register("req-1").expect("register");
        assert!(!token.is_cancelled());

        state.cancel("req-1").expect("cancel");
        assert!(token.is_cancelled(), "取消标志应被置位");

        state.unregister("req-1", &token).expect("unregister");
        assert!(
            state.0.lock().expect("lock").is_empty(),
            "结束后必须从 map 移除条目，避免泄漏"
        );
        // 已结束的请求再取消仍是幂等 no-op。
        state.cancel("req-1").expect("cancel after unregister");
    }

    #[test]
    fn duplicate_request_id_does_not_replace_the_live_token() {
        let state = PolishCancelState::default();
        let first = state.register("duplicate").expect("first register");

        assert!(
            state.register("duplicate").is_err(),
            "重复 request_id 必须拒绝，不能顶掉在途请求"
        );
        state.cancel("duplicate").expect("cancel");
        assert!(first.is_cancelled(), "取消仍应作用于原始 token");
        assert_eq!(state.0.lock().expect("lock").len(), 1);
    }

    #[test]
    fn unregister_checks_token_identity() {
        let state = PolishCancelState::default();
        let current = state.register("identity").expect("register");
        let stale = Arc::new(CancelToken::default());

        state.unregister("identity", &stale).expect("stale drop");
        let tokens = state.0.lock().expect("lock");
        assert!(
            tokens
                .get("identity")
                .is_some_and(|token| Arc::ptr_eq(token, &current)),
            "旧 guard 不得误删同 ID 的新注册"
        );
    }

    #[tokio::test]
    async fn dropping_the_command_future_releases_its_registration() {
        let state = PolishCancelState::default();
        let future = async {
            let _registration =
                CancelRegistration::new(&state, "dropped".to_string()).expect("register");
            futures_util::future::pending::<()>().await;
        };
        let mut future = Box::pin(future);

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut future)
                .await
                .is_err()
        );
        assert_eq!(state.0.lock().expect("lock").len(), 1);
        drop(future);
        assert!(
            state.0.lock().expect("lock").is_empty(),
            "future drop 必须通过 RAII 清理 token"
        );
    }

    fn ok_chunks(
        payloads: &[&str],
    ) -> impl Stream<Item = Result<Vec<u8>, std::convert::Infallible>> {
        let chunks: Vec<_> = payloads.iter().map(|p| Ok(p.as_bytes().to_vec())).collect();
        futures_util::stream::iter(chunks)
    }

    #[tokio::test]
    async fn response_body_reader_enforces_its_byte_cap() {
        let oversized = "x".repeat(128);
        let body = read_body_with_limit(ok_chunks(&[&oversized]), 32, None)
            .await
            .expect("bounded read");

        assert_eq!(body.len(), 33, "最多保留上限及一个超限检测字节");
    }

    #[tokio::test]
    async fn error_body_read_races_cancellation_while_upstream_drips() {
        let stream = ok_chunks(&["partial error"]).chain(futures_util::stream::pending::<
            Result<Vec<u8>, std::convert::Infallible>,
        >());
        let cancelled = Arc::new(CancelToken::default());
        let cancel_from_task = Arc::clone(&cancelled);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_from_task.cancel();
        });

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            read_body_with_limit(stream, MAX_ERROR_BODY_BYTES, Some(&cancelled)),
        )
        .await
        .expect("慢速错误体必须在取消后立即释放请求");
        assert_eq!(result.expect_err("应返回取消错误"), POLISH_CANCELLED);
    }

    #[tokio::test]
    async fn cancelled_flag_stops_the_stream_loop() {
        let stream = ok_chunks(&[
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n",
        ]);
        // 循环开始前就已取消：一个 chunk 都不应被消费。
        let cancelled = CancelToken::default();
        cancelled.cancel();
        let mut emitted = Vec::new();

        let err = consume_sse_stream(
            "deepseek",
            stream,
            &mut |delta: String| {
                emitted.push(delta);
                Ok(())
            },
            &cancelled,
        )
        .await
        .expect_err("取消不应被当成正常完成");

        assert_eq!(err, POLISH_CANCELLED);
        assert!(emitted.is_empty(), "已取消的流不应再推送 delta");
    }

    #[tokio::test]
    async fn cancellation_mid_stream_stops_after_current_chunk() {
        let stream = ok_chunks(&[
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n",
        ]);
        let cancelled = Arc::new(CancelToken::default());
        let cancel_from_emit = Arc::clone(&cancelled);
        let mut emitted = Vec::new();

        let err = consume_sse_stream(
            "deepseek",
            stream,
            &mut |delta: String| {
                // 模拟前端在收到第一个 delta 后立刻点了取消。
                cancel_from_emit.cancel();
                emitted.push(delta);
                Ok(())
            },
            &cancelled,
        )
        .await
        .expect_err("中途取消不应返回截断内容");

        assert_eq!(err, POLISH_CANCELLED);
        assert_eq!(emitted, vec!["a".to_string()], "取消后不应再消费后续 chunk");
    }

    #[tokio::test]
    async fn cancel_interrupts_a_stream_that_is_waiting_for_data() {
        // 回归测试：旧实现只在收到下一个 chunk 后检查 AtomicBool，
        // 上游静默时取消要等到 read timeout(120s) 才生效。
        let cancelled = Arc::new(CancelToken::default());
        let cancel_from_task = Arc::clone(&cancelled);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_from_task.cancel();
        });

        let stream = futures_util::stream::pending::<Result<Vec<u8>, std::convert::Infallible>>();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            consume_sse_stream("deepseek", stream, &mut |_| Ok(()), &cancelled),
        )
        .await
        .expect("取消应在一秒内生效，而不是等 read timeout");

        assert_eq!(result.expect_err("应返回取消错误"), POLISH_CANCELLED);
    }

    #[tokio::test]
    async fn upstream_error_after_delta_fails_instead_of_returning_a_prefix() {
        let stream = ok_chunks(&[
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
            "data: {\"error\":{\"message\":\"upstream failed\"}}\n\n",
        ]);
        let cancelled = CancelToken::default();
        let mut emitted = Vec::new();

        let error = consume_sse_stream(
            "deepseek",
            stream,
            &mut |delta: String| {
                emitted.push(delta);
                Ok(())
            },
            &cancelled,
        )
        .await
        .expect_err("上游 error 不得被当成正常完成");

        assert!(error.contains("upstream failed"));
        assert_eq!(emitted, vec!["partial".to_string()]);
    }

    #[tokio::test]
    async fn openai_length_and_content_filter_fail_after_delta() {
        for (reason, expected_error) in [("length", "长度上限"), ("content_filter", "内容安全策略")]
        {
            let terminal = format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":\"tail\"}},\"finish_reason\":\"{reason}\"}}]}}\n\n"
            );
            let stream = ok_chunks(&[
                "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
                &terminal,
                "data: [DONE]\n\n",
            ]);
            let cancelled = CancelToken::default();
            let mut emitted = Vec::new();

            let error = consume_sse_stream(
                "deepseek",
                stream,
                &mut |delta: String| {
                    emitted.push(delta);
                    Ok(())
                },
                &cancelled,
            )
            .await
            .expect_err("截断或过滤不得返回已生成的前缀");

            assert!(error.contains(expected_error), "{reason}: {error}");
            assert_eq!(
                emitted,
                vec!["partial".to_string()],
                "异常终止帧里的 final delta 也不应作为正常文本发出"
            );
        }
    }

    #[tokio::test]
    async fn anthropic_max_tokens_and_refusal_fail_after_delta() {
        for (reason, expected_error) in [("max_tokens", "长度上限"), ("refusal", "拒绝生成")]
        {
            let terminal = format!(
                "data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"{reason}\"}}}}\n\n"
            );
            let stream = ok_chunks(&[
                "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"partial\"}}\n\n",
                &terminal,
                "data: {\"type\":\"message_stop\"}\n\n",
            ]);
            let cancelled = CancelToken::default();
            let mut emitted = Vec::new();

            let error = consume_sse_stream(
                "anthropic",
                stream,
                &mut |delta: String| {
                    emitted.push(delta);
                    Ok(())
                },
                &cancelled,
            )
            .await
            .expect_err("截断或拒绝不得因后续 message_stop 而成功");

            assert!(error.contains(expected_error), "{reason}: {error}");
            assert_eq!(emitted, vec!["partial".to_string()]);
        }
    }

    #[tokio::test]
    async fn eof_without_a_provider_completion_signal_is_an_error() {
        for (provider, payload) in [
            (
                "deepseek",
                "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
            ),
            (
                "anthropic",
                "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"partial\"}}\n\n",
            ),
        ] {
            let cancelled = CancelToken::default();
            let error =
                consume_sse_stream(provider, ok_chunks(&[payload]), &mut |_| Ok(()), &cancelled)
                    .await
                    .expect_err("提前 EOF 不得返回部分结果");

            assert!(error.contains("提前结束"), "provider={provider}: {error}");
        }
    }

    #[tokio::test]
    async fn openai_done_marker_finishes_normally_without_a_trailing_newline() {
        let stream = ok_chunks(&[
            "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n",
            "data: [DONE]",
        ]);
        let cancelled = CancelToken::default();
        let mut emitted = Vec::new();

        let content = consume_sse_stream(
            "deepseek",
            stream,
            &mut |delta: String| {
                emitted.push(delta);
                Ok(())
            },
            &cancelled,
        )
        .await
        .expect("正常结束");

        assert_eq!(content, "hello world");
        assert_eq!(emitted, vec!["hello ".to_string(), "world".to_string()]);
    }

    #[tokio::test]
    async fn openai_finish_reason_preserves_the_final_delta() {
        let stream = ok_chunks(&[
            "data: {\"choices\":[{\"delta\":{\"content\":\"tail\"},\"finish_reason\":\"stop\"}]}\n\n",
        ]);
        let cancelled = CancelToken::default();

        let content = consume_sse_stream("deepseek", stream, &mut |_| Ok(()), &cancelled)
            .await
            .expect("finish_reason 是正常结束标记");

        assert_eq!(content, "tail");
    }

    #[tokio::test]
    async fn anthropic_message_stop_finishes_normally() {
        let stream = ok_chunks(&[
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"答案\"}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        ]);
        let cancelled = CancelToken::default();

        let content = consume_sse_stream("anthropic", stream, &mut |_| Ok(()), &cancelled)
            .await
            .expect("message_stop 是正常结束标记");

        assert_eq!(content, "答案");
    }

    #[tokio::test]
    async fn multiline_sse_data_is_aggregated_at_blank_lines() {
        let stream = ok_chunks(&[concat!(
            "event: message\n",
            "data: {\"choices\":[\n",
            "data: {\"delta\":{\"content\":\"多行\"},\"finish_reason\":null}\n",
            "data: ]}\n\n",
            "data: [DONE]\n\n"
        )]);
        let cancelled = CancelToken::default();

        let content = consume_sse_stream("deepseek", stream, &mut |_| Ok(()), &cancelled)
            .await
            .expect("多行 data 应拼接成一个 JSON 事件");

        assert_eq!(content, "多行");
    }
}

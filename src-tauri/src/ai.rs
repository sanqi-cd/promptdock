use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::ipc::Channel;
use uuid::Uuid;

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

fn build_client() -> Result<Client, String> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
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

    let is_anthropic = settings.provider.to_lowercase().contains("anthropic");

    let request_body = if is_anthropic {
        serde_json::json!({
            "model": settings.model,
            "max_tokens": 4096,
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

    let mut req = client
        .post(&endpoint)
        .header("Content-Type", "application/json");

    if is_anthropic {
        req = req.header("x-api-key", api_key);
    } else {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

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

fn parse_openai_stream_delta(json: &Value) -> Option<String> {
    json["choices"][0]["delta"]["content"]
        .as_str()
        .map(str::to_string)
}

fn emit_polish_delta(
    on_chunk: &Channel<PolishStreamEvent>,
    delta: String,
) -> Result<(), String> {
    on_chunk
        .send(PolishStreamEvent::Delta { delta })
        .map_err(|e| format!("流式结果发送失败: {}", e))
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

    let is_anthropic = settings.provider.to_lowercase().contains("anthropic");
    if is_anthropic {
        let content = chat_completion(settings, system_prompt, user_message).await?;
        emit_polish_delta(&on_chunk, content.clone())?;
        on_chunk
            .send(PolishStreamEvent::Done {
                content: content.clone(),
            })
            .map_err(|e| format!("流式结果发送失败: {}", e))?;
        return Ok(content);
    }

    let client = build_client()?;
    let request_body = serde_json::json!({
        "model": settings.model,
        "stream": true,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_message}
        ]
    });
    let endpoint = format!(
        "{}/chat/completions",
        settings.base_url.trim_end_matches('/')
    );

    let response = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
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
    let mut pending = String::new();
    let mut content = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取流式响应失败: {}", e))?;
        pending.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline_index) = pending.find('\n') {
            let raw_line = pending[..newline_index].trim().to_string();
            pending = pending[newline_index + 1..].to_string();

            if raw_line.is_empty() || raw_line.starts_with(':') {
                continue;
            }

            let Some(data) = raw_line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                on_chunk
                    .send(PolishStreamEvent::Done {
                        content: content.clone(),
                    })
                    .map_err(|e| format!("流式结果发送失败: {}", e))?;
                return Ok(content);
            }

            let json: Value = serde_json::from_str(data)
                .map_err(|e| format!("解析流式响应失败: {}", e))?;
            if let Some(delta) = parse_openai_stream_delta(&json) {
                if !delta.is_empty() {
                    content.push_str(&delta);
                    emit_polish_delta(&on_chunk, delta)?;
                }
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

fn polish_system_prompt(mode: &str) -> &'static str {
    match mode {
        "structured" => STRUCTURED_POLISH_PROMPT,
        "professional" => EXPERT_POLISH_PROMPT,
        "concise" => CONCISE_POLISH_PROMPT,
        "general" => OPTIMIZE_POLISH_PROMPT,
        _ => OPTIMIZE_POLISH_PROMPT,
    }
}

fn polish_mode_label(mode: &str) -> &'static str {
    match mode {
        "structured" => "结构化",
        "professional" => "专家增强",
        "concise" => "精简",
        "general" => "优化",
        _ => "优化",
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

const POLISH_PROCESS_PROMPT: &str = r#"你是 PromptDock 的「可展示处理要点」生成器。

任务：
根据用户原始 Prompt 和所选润色模式，生成本次 AI 将如何处理这个 Prompt 的 3-4 条可展示要点。

重要边界：
- 这些要点是给用户看的“处理计划/优化方向”，不是隐藏思维链。
- 不要输出逐步推理、内心分析、长篇解释或最终润色结果。
- 必须结合用户原文中的具体任务，不要返回通用套话。
- 不要编造用户没有提供的事实。

JSON 格式要求：
[
  { "title": "不超过8个汉字", "desc": "不超过28个汉字，说明本步会处理什么" }
]

只返回 JSON 数组，不要包含 Markdown、代码块或其他文本。"#;

const OPTIMIZE_POLISH_PROMPT: &str = r#"你是 PromptDock 的「优化」skill，负责把用户输入改写成更清晰、具体、可执行的 Prompt。

适合场景：
- 用户的表达偏口语化、模糊、缺少关键条件。
- 用户想提升一次生成质量，但不需要完整工作流。

内部处理流程：
1. 识别用户真实意图和任务类型，例如写作、代码、分析、图像、营销、学习、汇报、产品设计或其他。
2. 判断原 Prompt 缺少哪些会影响执行质量的信息：目标对象、主题范围、输出形式、语气风格、限制条件、评价标准。
3. 在不编造事实的前提下补足必要执行条件；无法确定的信息用 [待补充] 占位。
4. 将口语化表达改写成可直接交给 AI 执行的 Prompt。

边界：
- 不大幅扩写，不把简单任务改造成复杂方案。
- 不强制结构化，不添加用户没有暗示的业务背景。
- 不改变用户原意。

输出规则：
- 直接返回优化后的 Prompt，不要解释。
- 保留用户原始目标，补足必要对象、范围、语气、格式或限制。
- 让模型拿到结果后能立刻执行。"#;

const STRUCTURED_POLISH_PROMPT: &str = r#"你是 PromptDock 的「结构化」skill，负责把散乱需求整理成模块化 Prompt。

适合场景：
- 用户提供了多个要求、背景、条件或输出格式，但信息组织松散。
- 用户希望 Prompt 更容易复用和二次编辑。

内部处理流程：
1. 识别用户真实意图、任务类型和已有信息块。
2. 合并重复要求，把背景、目标、约束、输出格式、素材和注意事项分组。
3. 根据输入复杂度选择章节；简单输入使用轻结构，复杂输入使用完整结构。
4. 对缺失但必要的信息使用 [待补充]，不要编造具体事实。

边界：
- 重点是组织信息，不做不必要扩写。
- 不把任务升级成专家方案，除非用户已经给出明确专业场景。
- 不要机械套用所有章节，避免空壳标题。

输出规则：
- 直接返回结构化后的 Prompt，不要解释。
- 使用清晰章节组织内容，优先包含：角色、任务、背景、约束、输出格式、注意事项。
- 可以使用标题和要点，但避免空泛模板。"#;

const CONCISE_POLISH_PROMPT: &str = r#"你是 PromptDock 的「精简」skill，负责压缩 Prompt，保留核心诉求和关键信息。

适合场景：
- 用户输入过长、重复、夹杂聊天语气或低价值背景。
- 用户希望得到更短、更可复用、更省 token 的 Prompt。

内部处理流程：
1. 识别用户真实意图、任务类型和不可删除的关键条件。
2. 删除寒暄、重复表达、弱相关背景、犹豫语气和多余修饰。
3. 合并相近要求，保留任务目标、关键对象、必要限制、输出格式和质量底线。
4. 如果原文很长，优先压缩到原长度的 40%-70%；如果原文很短，只做去噪和轻量压缩。

边界：
- 不要扩写。
- 不新增角色设定、复杂步骤或额外输出要求。
- 不删除会影响结果质量的关键条件。
- 不改变任务类型。

输出规则：
- 直接返回精简后的 Prompt，不要解释。
- 保留核心诉求、关键对象、必要约束和输出格式。
- 删除客套话、重复表达、弱相关背景和多余修饰。"#;

const EXPERT_POLISH_PROMPT: &str = r#"你是 PromptDock 的「专家增强」skill，负责把用户需求升级为专家级 Prompt，用于正式工作和高质量复杂任务。

适合场景：
- 正式工作中的报告、方案、分析、代码审查、产品设计、营销策划、研究任务。
- 用户需要更稳定、更专业、更可控的输出。

内部处理流程：
1. 识别任务类型，并选择合适的专家框架：报告、方案、分析、代码审查、营销、产品设计、研究、图像生成或其他。
2. 提炼用户真实目标，判断缺失的关键上下文、受众、交付物、约束、评价标准和风险边界。
3. 在不编造事实的前提下补充合理的角色、执行步骤、输出格式和质量标准。
4. 对不确定信息使用 [待补充]，让用户后续可编辑；不要编造业务细节、数据、案例或用户未提供的事实。
5. 输出一份能用于正式工作的专家级 Prompt。

边界：
- 这是进阶增强，不适合简单翻译、短文案、一次性小任务。
- 可以补充合理的角色、背景、步骤和质量标准，但不能虚构具体事实。
- 若用户原始信息不足，用通用但可执行的约束表达，不要编造业务细节。

输出规则：
- 直接返回增强后的 Prompt，不要解释。
- 明确包含任务角色、目标、背景假设、执行步骤、输出格式、约束条件和质量标准。
- 输出应像一份可交给专家模型执行的正式任务说明。"#;

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

    let system_prompt = r#"你是一个提示词分类专家。分析用户给出的提示词，返回严格的 JSON 格式结果。

可选的分类有：内容创作、代码开发、学习研究、销售营销、数据分析、图像生成、工作汇报、产品设计、视频脚本、其他

标签规则：最多5个标签，每个不超过15字，使用中文。

JSON 格式要求：
{
  "category": "分类名称",
  "tags": ["标签1", "标签2", "标签3"],
  "summary": "一句话摘要，不超过50字",
  "recommended_tool": "推荐的AI工具：ChatGPT / Claude / DeepSeek / Midjourney / Gemini / Cursor / Other"
}

只返回 JSON，不要包含其他内容。"#;

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
    fn polish_skill_prompts_define_clear_boundaries() {
        let optimize = polish_system_prompt("general");
        assert!(optimize.contains("不大幅扩写"));
        assert!(optimize.contains("不改变用户原意"));
        assert!(optimize.contains("识别用户真实意图"));
        assert!(optimize.contains("[待补充]"));

        let structured = polish_system_prompt("structured");
        assert!(structured.contains("角色"));
        assert!(structured.contains("任务"));
        assert!(structured.contains("输出格式"));
        assert!(structured.contains("根据输入复杂度选择章节"));
        assert!(structured.contains("不要机械套用所有章节"));

        let concise = polish_system_prompt("concise");
        assert!(concise.contains("不要扩写"));
        assert!(concise.contains("保留核心诉求"));
        assert!(concise.contains("40%-70%"));
        assert!(concise.contains("不改变任务类型"));

        let expert = polish_system_prompt("professional");
        assert!(expert.contains("专家级"));
        assert!(expert.contains("质量标准"));
        assert!(expert.contains("正式工作"));
        assert!(expert.contains("识别任务类型"));
        assert!(expert.contains("不要编造业务细节"));
        assert!(expert.contains("[待补充]"));
    }

    #[test]
    fn unknown_polish_skill_falls_back_to_optimize() {
        assert_eq!(
            polish_system_prompt("missing"),
            polish_system_prompt("general")
        );
    }

    #[test]
    fn polish_process_prompt_requests_visible_steps_not_hidden_reasoning() {
        assert!(POLISH_PROCESS_PROMPT.contains("可展示要点"));
        assert!(POLISH_PROCESS_PROMPT.contains("不是隐藏思维链"));
        assert!(POLISH_PROCESS_PROMPT.contains("必须结合用户原文"));
        assert!(POLISH_PROCESS_PROMPT.contains("只返回 JSON 数组"));
    }

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
}

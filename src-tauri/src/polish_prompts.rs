//! 各 AI 能力的系统提示词。
//!
//! 与调用逻辑分开放置：这里是纯文本内容，改动的性质是「调优提示词」，
//! 不应该和 HTTP / 流式解析的改动混在同一个文件里。

/// 「可展示处理要点」生成器。
///
/// 产出的是给用户看的处理计划，**不是**隐藏思维链 —— 提示词正文里对此有明确边界。
pub const POLISH_PROCESS_PROMPT: &str = r#"你是 PromptDock 的「可展示处理要点」生成器。

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

/// 自动分类的系统提示词。分类枚举同时被 `categoryIcons` 前端使用，改动需同步。
pub const CLASSIFY_SYSTEM_PROMPT: &str = r#"你是一个提示词分类专家。分析用户给出的提示词，返回严格的 JSON 格式结果。

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

/// 把用户选择的润色模式映射到对应的系统提示词；未知模式回退到「优化」。
pub fn polish_system_prompt(mode: &str) -> &'static str {
    match mode {
        "structured" => STRUCTURED_POLISH_PROMPT,
        "professional" => EXPERT_POLISH_PROMPT,
        "concise" => CONCISE_POLISH_PROMPT,
        "general" => OPTIMIZE_POLISH_PROMPT,
        _ => OPTIMIZE_POLISH_PROMPT,
    }
}

/// 在「处理要点」请求中向模型描述当前模式。
pub fn polish_mode_label(mode: &str) -> &'static str {
    match mode {
        "structured" => "结构化",
        "professional" => "专家增强",
        "concise" => "精简",
        "general" => "优化",
        _ => "优化",
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
    fn polish_mode_labels_cover_every_mode() {
        for (mode, label) in [
            ("general", "优化"),
            ("structured", "结构化"),
            ("concise", "精简"),
            ("professional", "专家增强"),
        ] {
            assert_eq!(polish_mode_label(mode), label);
        }
        assert_eq!(polish_mode_label("missing"), "优化");
    }

    #[test]
    fn polish_process_prompt_requests_visible_steps_not_hidden_reasoning() {
        assert!(POLISH_PROCESS_PROMPT.contains("可展示要点"));
        assert!(POLISH_PROCESS_PROMPT.contains("不是隐藏思维链"));
        assert!(POLISH_PROCESS_PROMPT.contains("必须结合用户原文"));
        assert!(POLISH_PROCESS_PROMPT.contains("只返回 JSON 数组"));
    }

    #[test]
    fn classify_prompt_lists_the_categories_the_ui_expects() {
        // 分类枚举与前端 CategoryTags / categoryIcons 共用，改动需同步。
        for category in [
            "内容创作",
            "代码开发",
            "学习研究",
            "销售营销",
            "数据分析",
            "图像生成",
            "工作汇报",
            "产品设计",
            "视频脚本",
            "其他",
        ] {
            assert!(
                CLASSIFY_SYSTEM_PROMPT.contains(category),
                "分类提示词缺少 {category}"
            );
        }
    }
}

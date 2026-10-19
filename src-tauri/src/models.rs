use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: String,
    pub title: String,
    pub content: String,
    pub optimized_content: Option<String>,
    pub category: Option<String>,
    pub summary: Option<String>,
    pub is_favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct PromptWithTags {
    pub prompt: Prompt,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub is_active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: String,
    pub is_favorite: bool,
}

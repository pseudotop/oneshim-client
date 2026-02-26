use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone)]
pub struct TagInfo {
    pub id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_search_type")]
    pub search_type: String,
    pub tag_ids: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

fn default_search_type() -> String {
    "all".to_string()
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub result_type: String,
    pub id: String,
    pub timestamp: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub matched_text: Option<String>,
    pub image_url: Option<String>,
    pub importance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<TagInfo>>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub total: u64,
    pub offset: usize,
    pub limit: usize,
    pub results: Vec<SearchResult>,
}

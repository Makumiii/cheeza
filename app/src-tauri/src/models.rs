use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub parent_path: String,
    pub name: String,
    pub aspect_ratio: String,
    pub platform_target: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBlockInput {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTrayItemInput {
    pub id: String,
    pub playback_mode: String,
    pub in_point_us: i64,
    pub out_point_us: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub id: String,
    pub name: String,
    pub path: String,
    pub aspect_ratio: String,
    pub platform_target: String,
    pub script: String,
    pub blocks: Vec<ScriptBlock>,
    pub assets: Vec<MediaAsset>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptBlock {
    pub id: String,
    pub position: i64,
    pub text: String,
    pub status: String,
    pub alignment_stale: bool,
    pub tray: Vec<TrayItem>,
    pub takes: Vec<Take>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Take {
    pub id: String,
    pub relative_path: String,
    pub processed_relative_path: Option<String>,
    pub duration_us: i64,
    pub selected: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub media_type: String,
    pub content_hash: String,
    pub duration_us: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub proxy_relative_path: Option<String>,
    pub thumbnail_relative_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayItem {
    pub id: String,
    pub asset_id: String,
    pub position: i64,
    pub playback_mode: String,
    pub in_point_us: i64,
    pub out_point_us: Option<i64>,
}

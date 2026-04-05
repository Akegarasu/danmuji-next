//! Bilibili 视频信息获取模块

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// 视频信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    /// BV 号
    pub bvid: String,
    /// AV 号
    pub aid: u64,
    /// 标题
    pub title: String,
    /// 封面 URL
    pub cover: String,
    /// 播放量
    pub view: u64,
    /// UP 主名称
    pub owner_name: String,
    /// UP 主头像
    pub owner_face: String,
    /// 视频时长（秒）
    pub duration: u64,
}

/// Bilibili 视频信息 API 响应
#[derive(Debug, Deserialize)]
struct ApiResponse {
    code: i32,
    message: String,
    data: Option<ApiVideoData>,
}

#[derive(Debug, Deserialize)]
struct ApiVideoData {
    bvid: String,
    aid: u64,
    title: String,
    #[serde(rename = "pic")]
    cover: String,
    stat: ApiStat,
    owner: ApiOwner,
    duration: u64,
}

#[derive(Debug, Deserialize)]
struct ApiStat {
    view: u64,
}

#[derive(Debug, Deserialize)]
struct ApiOwner {
    name: String,
    face: String,
}

/// 全局复用的 HTTP 客户端（避免每次请求创建新的连接池）
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .expect("Failed to build HTTP client")
});

/// 获取视频信息
/// video_id 支持 BV号 或 av号（如 "BV1xx..." 或 "av12345"）
pub async fn fetch_video_info(video_id: &str) -> Result<VideoInfo, String> {
    let url = if video_id.to_uppercase().starts_with("BV") {
        format!(
            "https://api.bilibili.com/x/web-interface/view?bvid={}",
            video_id
        )
    } else if let Some(av_num) = video_id
        .to_lowercase()
        .strip_prefix("av")
        .and_then(|s| s.parse::<u64>().ok())
    {
        format!(
            "https://api.bilibili.com/x/web-interface/view?aid={}",
            av_num
        )
    } else {
        return Err(format!("无效的视频 ID: {}", video_id));
    };

    let resp: ApiResponse = HTTP_CLIENT
        .get(&url)
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    if resp.code != 0 {
        return Err(format!("API 错误: {}", resp.message));
    }

    let data = resp.data.ok_or("视频数据为空")?;

    Ok(VideoInfo {
        bvid: data.bvid,
        aid: data.aid,
        title: data.title,
        cover: data.cover,
        view: data.stat.view,
        owner_name: data.owner.name,
        owner_face: data.owner.face,
        duration: data.duration,
    })
}

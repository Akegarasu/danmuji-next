//! Bilibili HTTP API 调用

mod cookie;
mod danmu;
mod moderation;
mod rank;
mod room;
mod wbi;

pub use cookie::{extract_buvid_from_cookie, extract_uid_from_cookie};
pub use danmu::{DanmuHost, DanmuServerInfo, get_danmu_info};
pub use moderation::{
    ShieldKeyword, ShieldKeywordListResponse, SilentUserResponse, add_shield_keyword,
    add_silent_user, del_shield_keyword, get_shield_keyword_list,
};
pub use rank::{ContributionRankResponse, ContributionRankUser, get_contribution_rank};
pub use room::{RoomInfo, get_room_init};

/// 默认 User-Agent (与 blivedm-go 保持一致)
pub(crate) const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:137.0) Gecko/20100101 Firefox/137.0";

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ApiResponse<T> {
    pub(crate) code: i32,
    pub(crate) message: String,
    pub(crate) data: T,
}

/// 基础响应（用于先检查 code）
#[derive(Debug, serde::Deserialize)]
pub(crate) struct BaseResponse {
    pub(crate) code: i32,
    pub(crate) message: String,
}

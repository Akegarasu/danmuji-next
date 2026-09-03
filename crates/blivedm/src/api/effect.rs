use std::collections::HashMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use super::{ApiResponse, USER_AGENT};
use crate::error::{Error, Result};

/// Bilibili 网页端使用的礼物全屏特效配置接口。
///
/// `roomGiftList` 返回的礼物配置中的 `effect_id` 对应这里的 `id`，而
/// `bind_gift_ids` 则给出该特效绑定的一个或多个礼物 ID。
pub const GIFT_EFFECT_CONFIG_URL: &str =
    "https://api.live.bilibili.com/xlive/general-interface/v1/fullScSpecialEffect/GetEffectConfListV2";

/// 礼物全屏特效配置响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftEffectConfig {
    pub full_sc_resource: GiftEffectResourceSet,
    #[serde(default)]
    pub float_sc_resource: Vec<FloatScResource>,
}

/// 全屏特效资源集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftEffectResourceSet {
    #[serde(default)]
    pub conf_list: Vec<GiftEffectResource>,
    #[serde(default)]
    pub base_version: u64,
    #[serde(default)]
    pub ttl: u64,
}

/// 单个礼物全屏特效资源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftEffectResource {
    #[serde(rename = "type")]
    pub effect_type: u8,
    #[serde(default)]
    pub web_mp4: String,
    #[serde(default)]
    pub web_mp4_json: String,
    #[serde(default)]
    pub horizontal_mp4: String,
    #[serde(default)]
    pub vertical_mp4: String,
    pub id: u64,
    #[serde(default)]
    pub plan_platform: Vec<u8>,
    #[serde(default)]
    pub bind_gift_ids: Vec<u64>,
    #[serde(default)]
    pub web_mp4_md5: String,
    #[serde(default)]
    pub horizontal_mp4_md5: String,
    #[serde(default)]
    pub vertical_mp4_md5: String,
    #[serde(default)]
    pub web_mp4_crc32: u64,
    #[serde(default)]
    pub horizontal_mp4_crc32: u64,
    #[serde(default)]
    pub vertical_mp4_crc32: u64,
    #[serde(default)]
    pub web_mp4_file_size: u64,
    #[serde(default)]
    pub horizontal_mp4_file_size: u64,
    #[serde(default)]
    pub vertical_mp4_file_size: u64,
    #[serde(default)]
    pub h265_conf: Option<GiftEffectH265Config>,
    #[serde(default)]
    pub online_time: u64,
}

/// H.265 版本的特效视频资源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftEffectH265Config {
    #[serde(default)]
    pub horizontal_mp4: Option<GiftEffectVideoVariant>,
    #[serde(default)]
    pub vertical_mp4: Option<GiftEffectVideoVariant>,
}

/// H.265 视频资源的校验信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiftEffectVideoVariant {
    #[serde(rename = "mp4", default)]
    pub url: String,
    #[serde(default)]
    pub mp4_md5: String,
    #[serde(default)]
    pub mp4_json: String,
    #[serde(default)]
    pub mp4_crc32: u64,
    #[serde(default)]
    pub mp4_file_size: u64,
}

/// 浮层 SC 资源。它与礼物全屏视频不是同一类资源，但接口会一并返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatScResource {
    #[serde(default)]
    pub title: String,
    #[serde(rename = "type", default)]
    pub resource_type: u8,
    #[serde(default)]
    pub left_color: String,
    #[serde(default)]
    pub right_color: String,
    #[serde(default)]
    pub face_background: String,
    #[serde(default)]
    pub tail_background: String,
    #[serde(default)]
    pub id: u64,
}

impl GiftEffectConfig {
    /// 按 Bilibili 特效 ID 查找资源。
    pub fn effect_for_id(&self, effect_id: u64) -> Option<&GiftEffectResource> {
        self.full_sc_resource
            .conf_list
            .iter()
            .find(|effect| effect.id == effect_id)
    }

    /// 按 Bilibili 礼物 ID 查找绑定的全屏特效。
    pub fn effect_for_gift(&self, gift_id: u64) -> Option<&GiftEffectResource> {
        self.full_sc_resource
            .conf_list
            .iter()
            .find(|effect| effect.bind_gift_ids.contains(&gift_id))
    }

    /// 构造礼物 ID 到特效资源的缓存映射。
    ///
    /// 一个特效可能绑定多个礼物，因此同一资源会被放入多个 key。接口偶尔
    /// 会返回重复绑定记录，`or_insert` 保留列表中较早的配置。
    pub fn gift_effect_map(&self) -> HashMap<u64, GiftEffectResource> {
        let mut map = HashMap::new();
        for effect in &self.full_sc_resource.conf_list {
            for gift_id in &effect.bind_gift_ids {
                if *gift_id != 0 {
                    map.entry(*gift_id).or_insert_with(|| effect.clone());
                }
            }
        }
        map
    }
}

#[derive(Debug, Deserialize)]
struct GiftEffectApiData {
    full_sc_resource: Option<GiftEffectResourceSet>,
    #[serde(default)]
    float_sc_resource: Vec<FloatScResource>,
}

/// 获取 Bilibili 礼物全屏特效配置。
///
/// 这是网页端使用的公开 GET 接口，不需要 Cookie。`area_parent_id`、
/// `area_id` 和 `base_version` 是可选的；传 0 可以获取房间通用配置。
/// `platform = pc` 时接口优先返回 `web_mp4` 和 `web_mp4_json`。
pub async fn get_gift_effect_config(
    client: &Client,
    room_id: u64,
    area_parent_id: Option<u64>,
    area_id: Option<u64>,
    base_version: Option<u64>,
) -> Result<GiftEffectConfig> {
    if room_id == 0 {
        return Err(Error::InvalidRoomId(room_id));
    }

    let mut url =
        Url::parse(GIFT_EFFECT_CONFIG_URL).map_err(|error| Error::Config(error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("platform", "pc")
        .append_pair("room_id", &room_id.to_string())
        .append_pair(
            "area_parent_id",
            &area_parent_id.unwrap_or_default().to_string(),
        )
        .append_pair("area_id", &area_id.unwrap_or_default().to_string())
        .append_pair("source", "live")
        .append_pair("build", "0")
        .append_pair(
            "base_version",
            &base_version.unwrap_or_default().to_string(),
        );

    let response: ApiResponse<GiftEffectApiData> = client
        .get(url.as_str())
        .header("User-Agent", USER_AGENT)
        .header("Referer", format!("https://live.bilibili.com/{room_id}"))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if response.code != 0 {
        return Err(Error::Api {
            code: response.code,
            message: response.message,
        });
    }

    let full_sc_resource = response
        .data
        .full_sc_resource
        .ok_or_else(|| Error::Config("Bilibili 未返回 full_sc_resource".to_string()))?;

    Ok(GiftEffectConfig {
        full_sc_resource,
        float_sc_resource: response.data.float_sc_resource,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> GiftEffectConfig {
        let data = json!({
            "full_sc_resource": {
                "base_version": 123,
                "ttl": 456,
                "conf_list": [{
                    "type": 1,
                    "web_mp4": "https://i0.hdslb.com/bfs/live/effect.mp4",
                    "web_mp4_json": "https://i0.hdslb.com/bfs/live/effect.json",
                    "horizontal_mp4": "",
                    "vertical_mp4": "",
                    "id": 2346,
                    "plan_platform": [1, 2],
                    "bind_gift_ids": [35195],
                    "web_mp4_md5": "md5",
                    "web_mp4_crc32": 400866571,
                    "web_mp4_file_size": 507202,
                    "h265_conf": null,
                    "online_time": 0
                }]
            },
            "float_sc_resource": []
        });
        serde_json::from_value(data).expect("valid effect fixture")
    }

    #[test]
    fn maps_gift_id_to_effect_resource() {
        let config = fixture();
        let effect = config.effect_for_gift(35195).expect("bound gift");

        assert_eq!(effect.id, 2346);
        assert_eq!(effect.web_mp4_file_size, 507202);
        assert_eq!(config.effect_for_id(2346).map(|item| item.id), Some(2346));
        assert!(config.effect_for_gift(1).is_none());
    }

    #[test]
    fn builds_a_gift_id_cache_map() {
        let config = fixture();
        let map = config.gift_effect_map();

        assert_eq!(map.get(&35195).map(|effect| effect.id), Some(2346));
        assert!(!map.contains_key(&0));
    }
}

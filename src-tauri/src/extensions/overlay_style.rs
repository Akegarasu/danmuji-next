//! OBS 外观配置：各扩展独立保存，复用同一份校验和浏览器样式协议。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverlayStyle {
    /// 留空使用各扩展原有字体；自定义值为一个本机字体名称。
    pub font_family: String,
    pub font_size: u16,
    pub font_weight: u16,
    pub text_color: String,
    pub secondary_color: String,
    pub shadow_enabled: bool,
    pub shadow_color: String,
    pub shadow_blur: u16,
    pub shadow_offset_x: i16,
    pub shadow_offset_y: i16,
}

impl OverlayStyle {
    pub fn overtime() -> Self {
        Self {
            font_family: String::new(),
            font_size: 96,
            font_weight: 400,
            text_color: "#ffffff".into(),
            secondary_color: "#ffffff".into(),
            shadow_enabled: true,
            shadow_color: "#000000".into(),
            shadow_blur: 10,
            shadow_offset_x: 0,
            shadow_offset_y: 0,
        }
    }

    pub fn song_request() -> Self {
        Self {
            font_size: 28,
            font_weight: 600,
            shadow_blur: 2,
            shadow_offset_y: 1,
            ..Self::overtime()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.font_family.chars().count() > 100 || self.font_family.chars().any(char::is_control)
        {
            return Err("字体名称最多 100 个字符，不能包含控制字符".into());
        }
        if !(12..=200).contains(&self.font_size) {
            return Err("OBS 字号须为 12–200 像素".into());
        }
        if !(100..=900).contains(&self.font_weight) || self.font_weight % 100 != 0 {
            return Err("OBS 字重须为 100–900 之间的整百数".into());
        }
        for color in [&self.text_color, &self.secondary_color, &self.shadow_color] {
            if color.len() != 7
                || !color.starts_with('#')
                || !color[1..].bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("OBS 颜色须为 #RRGGBB 格式".into());
            }
        }
        if self.shadow_blur > 50
            || !(-50..=50).contains(&self.shadow_offset_x)
            || !(-50..=50).contains(&self.shadow_offset_y)
        {
            return Err("阴影模糊须为 0–50 像素，偏移须为 -50–50 像素".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::{
        overtime::Overtime, song_request::SongRequestManager, Extension, ExtensionHost,
    };
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn legacy_checkpoints_keep_each_overlays_defaults() {
        let now = Instant::now();
        let extensions: [Box<dyn Extension>; 2] = [
            Box::new(Overtime::new(now)),
            Box::new(SongRequestManager::default()),
        ];
        for mut extension in extensions {
            let expected = extension.snapshot(now)["config"]["overlay_style"].clone();
            let mut legacy = extension.checkpoint(now);
            legacy["config"]
                .as_object_mut()
                .unwrap()
                .remove("overlay_style");
            extension.restore(legacy, now).unwrap();
            assert_eq!(extension.snapshot(now)["config"]["overlay_style"], expected);
        }
    }

    #[test]
    fn styles_persist_independently_and_broadcast_without_changing_runtime_state() {
        let directory = std::env::temp_dir().join(format!(
            "danmuji-overlay-style-{}",
            super::super::new_id("test")
        ));
        let host = ExtensionHost::new(directory.clone());
        for (id, size) in [("overtime", 80), ("song-request", 36)] {
            let mut receiver = host.subscribe(id).unwrap();
            let before = host.state(id).unwrap().state;
            let mut config = before["config"].clone();
            config["overlay_style"]["font_size"] = json!(size);
            config["overlay_style"]["font_family"] = json!("楷体");
            config["overlay_style"]["text_color"] = json!("#123456");
            host.request(id, json!({"type":"configure", "config":config}))
                .unwrap();
            assert!(receiver.has_changed().unwrap());
            let published = receiver.borrow_and_update().clone();
            let browser_style = if id == "overtime" {
                &published["config"]["overlay_style"]
            } else {
                &published["overlay_style"]
            };
            assert_eq!(browser_style, &config["overlay_style"]);
            let after = host.state(id).unwrap().state;
            for key in ["remaining_ms", "running", "rate", "requests", "session"] {
                assert_eq!(after[key], before[key]);
            }
            let mut invalid = config.clone();
            invalid["overlay_style"]["text_color"] = json!("red; background: red");
            assert!(host
                .request(id, json!({"type":"configure", "config":invalid}))
                .is_err());
            assert_eq!(host.state(id).unwrap().state, after);
            let restored = ExtensionHost::new(directory.clone());
            assert!(restored.last_error().is_none());
            assert_eq!(restored.state(id).unwrap().state["config"], config);
        }
        assert_eq!(
            host.state("overtime").unwrap().state["config"]["overlay_style"]["font_size"],
            80
        );
        assert_eq!(
            host.state("song-request").unwrap().state["config"]["overlay_style"]["font_size"],
            36
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn validates_style_limits_and_color_format() {
        let default = serde_json::to_value(OverlayStyle::overtime()).unwrap();
        for (key, value) in [
            ("font_size", json!(0)),
            ("font_size", json!(201)),
            ("font_weight", json!(450)),
            ("shadow_blur", json!(51)),
            ("shadow_offset_x", json!(-51)),
            ("shadow_offset_y", json!(51)),
            ("font_family", json!("bad\nfont")),
            ("font_family", json!("长".repeat(101))),
            ("text_color", json!("#fff")),
            ("secondary_color", json!("#gggggg")),
            ("shadow_color", json!("rgba(0,0,0,1)")),
        ] {
            let mut invalid = default.clone();
            invalid[key] = value;
            assert!(
                serde_json::from_value::<OverlayStyle>(invalid)
                    .unwrap()
                    .validate()
                    .is_err(),
                "{key}"
            );
        }
        let boundary = OverlayStyle {
            font_size: 200,
            font_weight: 900,
            shadow_blur: 50,
            shadow_offset_x: -50,
            shadow_offset_y: 50,
            ..OverlayStyle::song_request()
        };
        assert!(boundary.validate().is_ok());
    }
}

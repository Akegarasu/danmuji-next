//! Bilibili 弹幕协议库测试用例
//!
//! 运行方式：
//! ```bash
//! cd src-tauri
//! cargo run --example blivedm_test -- <room_id>
//! ```
//!
//! 示例：
//! ```bash
//! cargo run --example blivedm_test -- 21452505
//! ```

use std::env;

use danmuji_next_lib::blivedm::{BliveDmClient, Event};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 从命令行参数获取房间号
    let args: Vec<String> = env::args().collect();
    let room_id: u64 = if args.len() > 1 {
        args[1].parse().expect("请输入有效的房间号")
    } else {
        // 默认房间号（可以改成你想测试的房间）
        println!("用法: cargo run --example blivedm_test -- <room_id>");
        println!("未指定房间号，使用默认房间: 21452505");
        24160384
    };

    println!("========================================");
    println!("  Bilibili 弹幕协议库测试");
    println!("========================================");
    println!();

    // 构建客户端
    println!("📡 正在连接房间 {}...", room_id);

    let client = BliveDmClient::builder()
        .room_id(room_id)
        .auto_reconnect(true)
        .cookie("") // 可选：填入你的 Cookie 获取更完整的功能
        .build()
        .await?;

    let room_info = client.room_info();
    println!("✅ 房间信息获取成功:");
    println!("   - 房间 ID: {}", room_info.room_id);
    println!("   - 短号: {}", room_info.short_id);
    println!("   - 主播 UID: {}", room_info.uid);
    println!(
        "   - 直播状态: {}",
        if room_info.live_status == 1 {
            "🔴 直播中"
        } else {
            "⚫ 未开播"
        }
    );
    if !room_info.title.is_empty() {
        println!("   - 标题: {}", room_info.title);
    }
    println!();

    // 连接弹幕服务器
    println!("🔌 正在连接弹幕服务器...");
    let mut stream = client.connect().await?;
    println!("✅ 连接成功！开始接收消息...");
    println!();
    println!("========================================");
    println!("  实时消息");
    println!("========================================");

    // 接收消息
    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => print_event(&event),
            Err(e) => {
                eprintln!("❌ 错误: {}", e);
            }
        }
    }

    println!("连接已关闭");
    Ok(())
}

fn print_event(event: &Event) {
    match event {
        Event::Danmaku(dm) => {
            let medal_str = dm
                .sender
                .medal
                .as_ref()
                .map(|m| format!("[{}{}] ", m.name, m.level))
                .unwrap_or_default();

            let guard_str = match dm.sender.guard_level {
                danmuji_next_lib::blivedm::GuardLevel::Governor => "👑 ",
                danmuji_next_lib::blivedm::GuardLevel::Admiral => "⚓ ",
                danmuji_next_lib::blivedm::GuardLevel::Captain => "🚢 ",
                danmuji_next_lib::blivedm::GuardLevel::None => "",
            };

            println!(
                "💬 {}{}{}: {}",
                guard_str, medal_str, dm.sender.name, dm.content
            );
        }
        Event::Gift(gift) => {
            let value_str = if gift.is_paid() {
                let fen = gift.value_cny_fen();
                format!(" (¥{}.{:02})", fen / 100, fen % 100)
            } else {
                String::new()
            };

            println!(
                "🎁 {} {} {} x{}{}",
                gift.sender_name, gift.action, gift.gift_name, gift.num, value_str
            );
        }
        Event::SuperChat(sc) => {
            println!("💰 ========== SC ¥{} ==========", sc.price);
            println!("   {} 说:", sc.sender_name);
            println!("   {}", sc.message);
            println!("   ================================");
        }
        Event::GuardBuy(guard) => {
            println!(
                "⚓ {} 开通了 {} ({}个月)",
                guard.username,
                guard.guard_name(),
                guard.num
            );
        }
        Event::LiveStart(ls) => {
            println!();
            println!("🔴 ======== 开播了！ ========");
            println!();
        }
        Event::LiveStop(ls) => {
            println!();
            println!("⚫ ======== 下播了 ========");
            println!();
        }
        Event::Raw { cmd, .. } => {
            // 打印未处理的命令（调试用）
            log::debug!("📦 未处理的命令: {}", cmd);
        }
        Event::OnlineRankCount(online_rank_count) => {
            println!("👥 在线人数: {}", online_rank_count.online_count);
        }
        Event::OnlineRankV2(online_rank_v2) => {
            println!("👥 在线人数: {}", online_rank_v2.online_list.len());
        }
    }
}

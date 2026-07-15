# blivedm-rs

`blivedm-rs` 是从 danmuji-next 中拆分出的 Bilibili 直播 Rust 库，包含：

- WebSocket 弹幕连接、心跳与自动重连
- Zlib/Brotli 协议包解码
- 弹幕、礼物、醒目留言、大航海、开播/下播、在线排行和进房事件解析
- 房间信息、弹幕服务器、贡献排行和房管相关 HTTP API

贡献榜支持在线、日、周、月四种类型；大航海榜同时提供单页接口和自动遍历全部分页的接口：

```rust,no_run
use blivedm::api::{
    get_all_guard_top_list, get_contribution_rank_by_type, ContributionRankType,
};
# async fn example(client: &reqwest::Client) -> blivedm::Result<()> {
let daily = get_contribution_rank_by_type(
    client,
    12962,
    777964,
    None,
    ContributionRankType::Daily,
    1,
    100,
).await?;
let guards = get_all_guard_top_list(client, 12962, 777964, None).await?;
# Ok(())
# }
```

Cargo 包名是 `blivedm-rs`，Rust 库名是 `blivedm`。

## 在当前仓库中使用

```toml
[dependencies]
blivedm = { package = "blivedm-rs", path = "../crates/blivedm" }
```

```rust,no_run
use blivedm::{BliveDmClient, Event};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = BliveDmClient::builder()
        .room_id(21452505)
        .auto_reconnect(true)
        .build()
        .await?;

    let mut events = client.connect().await?;
    while let Some(event) = events.next().await {
        if let Event::Danmaku(danmaku) = event? {
            println!("{}: {}", danmaku.sender.name, danmaku.content);
        }
    }

    Ok(())
}
```

完整示例见 `examples/receive.rs`：

```bash
cargo run --example receive -- 21452505
```

## 原始事件观察

库本身不决定日志文件位置。调用方可通过 Builder 注入回调，用于调试、落盘或指标收集：

```rust,no_run
# use blivedm::BliveDmClient;
# async fn build() -> blivedm::Result<()> {
let client = BliveDmClient::builder()
    .room_id(21452505)
    .raw_event_handler(|value| println!("{value}"))
    .build()
    .await?;
# Ok(())
# }
```

## 发布状态

当前清单有意设置了 `publish = false`。仓库尚未声明开源许可证，发布前必须由项目所有者选择许可证；详见 `PUBLISHING.md`。

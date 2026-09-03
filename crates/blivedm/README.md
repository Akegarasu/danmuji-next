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

旧版 `raw_event_handler` 仍然保留，但它在协议解析任务中同步执行，只适合快速的
`try_send` 或调试输出，不应直接进行文件/数据库 I/O：

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

生产采集应使用有界、非阻塞的原始字节旁路。`EventNotifications` 会为成功解析为
`Event` 的 notification 尝试发送原始 body，并单独发送解码错误；原始旁路与业务事件
使用独立的有界通道，因此适合做可监控丢弃量的 best-effort 关联，但不提供事务性配对。
`Notifications` 会保留所有解压切包后的 notification 原始 JSON 字节和解码错误；
`AllFrames` 还会保留完整二进制 WS frame：

```rust,no_run
use blivedm::{BliveDmClient, RawCapture, RawCaptureMode};
use futures_util::StreamExt;
use tokio::sync::mpsc;

# async fn build() -> blivedm::Result<()> {
let (raw_tx, mut raw_rx) = mpsc::channel(1024);
let client = BliveDmClient::builder()
    .room_id(21452505)
    .raw_capture(raw_tx, RawCaptureMode::Notifications)
    .build()
    .await?;

tokio::spawn(async move {
    while let Some(raw) = raw_rx.recv().await {
        match raw {
            RawCapture::Notification { bytes } => {
                // 将 bytes 投递给调用方自己的 durable spool。
                let _ = bytes;
            }
            RawCapture::DecodeError { stage, error, .. } => {
                eprintln!("{stage:?}: {error}");
            }
            _ => {}
        }
    }
});

let mut events = client.connect_once().await?;
while let Some(event) = events.next().await {
    let _ = event?;
}
# Ok(())
# }
```

旁路只使用 `try_send`，通道满时不会阻塞 WebSocket 读取。可通过
`EventStream::dropped_raw_captures()` 监控丢弃数量。

## 受控单次连接与生命周期

`connect()` 的 Builder、返回类型和自动重连语义保持不变。它仍会在后台连接任务启动后
立即返回，因此 `is_connected()` 只表示事件通道尚未关闭，并不代表已经成功入房。

由上层 supervisor 管理 host/token 刷新、Cookie 轮换和退避时，使用 `connect_once()`；
通过状态 `watch` 或生命周期广播判断连接何时真正在线：

```rust,no_run
use blivedm::{BliveDmClient, ConnectionState, LifecycleEvent};
use tokio::sync::broadcast::error::RecvError;

# async fn connect() -> blivedm::Result<()> {
let client = BliveDmClient::builder().room_id(21452505).build().await?;
let mut events = client.connect_once().await?;
let mut lifecycle = events.take_lifecycle_receiver().expect("first receiver");
let cancellation = events.cancellation_token();

loop {
    match lifecycle.recv().await {
        Ok(LifecycleEvent::Online { trigger, .. }) => {
            println!("online, confirmed by {trigger:?}");
            break;
        }
        Ok(LifecycleEvent::Closed { reason, .. }) => {
            eprintln!("closed before online: {reason:?}");
            break;
        }
        Ok(_) => {}
        Err(RecvError::Lagged(skipped)) => {
            eprintln!("lifecycle receiver lagged by {skipped} events");
        }
        Err(RecvError::Closed) => break,
    }
}

assert!(matches!(events.state(), ConnectionState::Online { .. }));
cancellation.cancel();
# Ok(())
# }
```

默认连接参数为：握手 10 秒、入房确认 10 秒、心跳 30 秒、有效上行读空闲 75 秒。
`EnterRoomReply`、`HeartbeatReply`、首个合法 notification 均可确认在线；心跳回复携带的
人气值也会出现在生命周期事件中。丢弃 `EventStream` 会取消并终止唯一的连接任务，避免
残留心跳或读循环。

## 已知限制与暂缓方案

以下两项候选改进会改变可观察到的连接或投递时序，目前仅记录设计方向，**尚未实现**。
实施前需要单独审核行为兼容性。

### 业务事件背压会暂停连接循环

规范化业务事件通过有界通道无损、有序地发送。当消费者处理速度不足、通道写满时，
当前连接任务会等待通道容量；等待期间不会继续轮询 WebSocket、心跳 tick、入房超时或
读空闲超时。原始旁路自身仍使用 `try_send`，不会造成这种等待。

候选方案是：收到有效 frame 后先确认 Online 并重置读空闲计时，再按原顺序无损投递
业务事件；等待业务通道容量时继续处理心跳与取消。该方案会使 Online 状态相对首个
业务事件更早可见，因此暂缓实现。

### `EventNotifications` 不是事务性配对

当前实现会在 typed event 写入业务通道之前，以 `try_send` 投递对应的原始 body。
如果随后发生取消或业务消费者关闭，durable spool 末尾可能存在一条没有对应 typed
event 的原始记录；如果原始通道已满，则可能只有 typed event，并由
`dropped_raw_captures()` 计入丢弃数量。因此只能按 best-effort 方式关联，不能将两个
独立通道视为事务性 FIFO。

候选方案是仅在 typed event 成功进入业务通道后再投递原始 body。该方案会改变原始旁路
相对业务事件的投递时序，因此暂缓实现。

## 金额与幂等字段

`Gift::transaction_id` 保留上游 `SEND_GIFT.data.tid`，未提供时为 `None`，可用作礼物交易级幂等键。`batch_combo_id` 只表示连击批次，不是交易唯一 ID。

`SuperChat::price` 的上游单位是人民币元，因此 `value_cny_fen()` 按 `price * 100` 返回人民币分；礼物和大航海仍依上游金瓜子单位换算。`GuardBuy::price` 是 `GUARD_BUY` 给出的标准标价，不一定等于成交金额；实际订单总金额应使用 `GuardToast::price`（来自 `USER_TOAST_MSG` / `USER_TOAST_MSG_V2`），且该值已经包含购买数量，不应再乘 `num`。

## 发布状态

当前清单有意设置了 `publish = false`。仓库尚未声明开源许可证，发布前必须由项目所有者选择许可证；详见 `PUBLISHING.md`。

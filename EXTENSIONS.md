# 扩展运行层与 OBS 页面

## 边界

```text
blivedm 的 Gift / GiftBatch / GuardToast
  → LiveData 交易去重
  → ReceivedGift（单次数量，不是连击累计快照）
  → ExtensionHost → Extension 领域逻辑
       ├─ Tauri IPC：配置和操作、桌面状态查询
       ├─ watch 最新状态 → Axum SSE → OBS 独立静态页面
       └─ 本地 JSON 检查点
```

`src-tauri/src/extensions/` 不依赖 Tauri。宿主用统一的 `Extension` 接口管理注册、状态快照、配置操作、礼物分发和持久化。`Overtime` 只负责计时和规则，不连接 Bilibili，不持有 AppHandle，不依赖窗口订阅或 Pinia。

应用启动时创建宿主和本地服务，独立的每秒 tick 发布状态，每 5 秒落盘。计时以 Rust `Instant` 单调时钟为准，不用 tick 次数累计；窗口关闭、定时器调度延迟和多个浏览器连接不会加快或停止倒计时。重启读取检查点后暂停。设置先校验再修改；磁盘写入失败会明确提示操作已生效但尚未保存。

点播与投票保留现有 Manager / DataUpdate 协议，前端已统一到注册表。它们尚未迁移到新宿主，以避免更改现有点播抓取、投票和存档行为。新扩展可直接使用新的独立接口。

## 新增扩展

1. 在 `src-tauri/src/extensions/` 实现 `Extension`：固定 ID、`snapshot`、`checkpoint`、`restore`、`request`、`on_gift`。在 `ExtensionHost::new` 中注册。
2. 在 `src/components/extension/registry.ts` 注册桌面设置面板。`liveEvents` 仅供旧扩展使用，新宿主扩展可以留空。
3. 使用通用 IPC `get_extension_snapshot` 与 `extension_request`，参数为 `extensionId` 和 JSON 请求，传输适配放在 `src/services/`。
4. 如需 OBS 展示，提供独立 HTML / CSS / JS，在 `extensions/server.rs` 的静态资源表注册。共用下面的 HTTP/SSE 协议，不从桌面 App.vue 或 main.ts 启动。

目前为编译期注册的内置扩展，不执行外部下载的任意插件代码。未来新增弹幕/SC 订阅时应在宿主加入最小化的标准事件类型，避免把原始通知、登录态或直播聚合器传给插件。

## 只读 HTTP 协议

服务固定绑定 `127.0.0.1`，默认端口 `17654`。可在设置面板修改；冲突时明确报错，不自动变更已配置的 OBS 地址。资源使用 `include_bytes!` 嵌入可执行文件，开发和安装包均无需额外 Node 服务、Vite 进程或资源路径查找。

| 路径 | 内容 |
| --- | --- |
| `GET /api/extensions` | 协议版本 1 与已注册扩展 ID |
| `GET /api/extensions/{id}/state` | 完整状态 JSON |
| `GET /api/extensions/{id}/events` | SSE，事件名 `snapshot` |
| `GET /overlays/overtime/` | 加班机 OBS 页面 |

SSE 首次连接和重连均发送完整状态。`watch` 合并慢客户端积压，避免无限队列。加班机状态包含 `config`、`remaining_ms`、`running`、`rate` 和最近 20 条命中规则的礼物 `notices`。未命中礼物不会输出，礼物提示序号仅在本次应用运行期间有效。客户端初始连接、重连恢复状态及最新一条投喂提示，不逐条重放历史；连续连接期间用序号识别新提示。提示按原页面的 10 秒循环动画展示，队列每隔至少 11.5 秒切换；队列清空后最后一条继续循环。显示循环不修改后端时间，也不重新抽签。

HTTP 不提供控制写接口；配置和开始、暂停、清空等操作只能通过 Tauri IPC。Host 与 Origin 校验阻止外部页面读取本地接口，不开放 CORS。输出没有 Cookie、UID、头像或原始 Bilibili 消息。服务换端口或退出时通知 SSE 连接结束，再关闭监听。

## 加班机请求示例

```json
{"type":"apply","action":"add","value":60}
```

`type` 支持 `configure`（带 `config`）、`start`、`pause`、`reset`、`apply`（带 `action` / `value`）。

动作：`add` / `subtract`（秒）、`multiply` / `divide`（剩余时间倍率）、`set_time`（秒）、`set_rate`（倒计时速度倍率）、`clear`。礼物规则额外支持 `random`，手动 `apply` 不接受该动作。

规则字段：`id`、`enabled`、`gift_id`（可为 null）、`gift_name`、`action`、`value`、`per_gift`、`random_ranges`。规则数量上限为 100，`gift_id` 优先于名称，按列表顺序执行所有匹配规则。

随机规则将 `action` 设为 `random`，`value` 设为 0，`random_ranges` 列出 1～4 种允许的操作，各操作不可重复。例如：

```json
{
  "id": "random-heart",
  "enabled": true,
  "gift_id": null,
  "gift_name": "小心心",
  "action": "random",
  "value": 0,
  "per_gift": true,
  "random_ranges": [
    { "action": "add", "min": 10, "max": 60 },
    { "action": "subtract", "min": 5, "max": 30 },
    { "action": "multiply", "min": 1.2, "max": 2 },
    { "action": "divide", "min": 1.1, "max": 1.5 }
  ]
}
```

后端使用 `rand` 从操作中等概率抽选，再在闭区间内均匀抽值。加减以整数秒抽取；乘除以 0.01 为步长。每条通知对每条匹配规则只抽一次，按个执行时复用结果，避免礼包数量导致无限抽签循环。旧规则缺少 `random_ranges` 时默认空列表，维持固定操作。

每条 `notice.results` 记录实际 `action`、`value`、执行数量 `count`、是否随机 `random` 和本次实际变化 `delta_ms`；`notice.actions` 也只包含已抽出的具体动作。页面展示实际结果，不能自行抽签。

## 皮肤来源与开发

布局采用白色 96px 计时文字、18px 投喂提示、25px 圆点礼物规则，配合黑色文字阴影；规则区宽 500px，提示区宽 449px。OBS 页面裁切原 1920px 画布到 600px 主体并去除顶部 50px 留白，保留双列透明布局。原卡片背景资源已移除，字体沿用之前下载的有效文件。

`widgetItemDivRightToLeft` 的原动画为 `10s linear infinite`，从 `translateX(170%)` 到 `translateX(-200%)`。原脚本消费完队列后仍保留最后一个组件，因此最后一条提示持续滚动；本地页面保留此行为，同时在禁用、重置时主动清理。

## 验证

```sh
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
node --test scripts/test_overtime_overlay.mjs
```

Rust 测试覆盖单调计时、暂停、倍率、边界值、数量及规则过滤、重启恢复、连击去重到计时器的管线、HTTP 静态资源、SSE 重连和本机访问限制。随机规则测试覆盖边界、四种运算、数量、旧配置与具体抽签结果。Node 测试验证 OBS 入口的断线显示、快照校准、循环提示和队列切换、随机范围与结果展示、规则渲染和资源清理，不需要浏览器自动化。

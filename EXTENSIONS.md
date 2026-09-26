# 扩展运行层与 OBS 页面

## 边界

```text
blivedm 的弹幕 / SC / 礼物 / 大航海
  → LiveData：直播统计、礼物交易去重、归档
  → live_events：ReceivedText / ReceivedGift
  → ExtensionHost：点播、投票、加班机
       ├─ ExtensionState → extension-state:{id} → 独立扩展客户端
       ├─ ExtensionEffect → tasks → 异步结果回到原扩展
       ├─ commands/extensions：统一状态查询、操作、只读业务查询
       ├─ watch 最新状态 → Axum SSE → OBS 独立静态页面
       └─ storage：extensions/{id}.json
```

`src-tauri/src/extensions/` 不依赖 Tauri、`LiveData`、`DataUpdate` 或窗口订阅。三个扩展都实现统一的 `Extension` 接口。领域实现只修改状态并返回是否变化，宿主记录版本与待发布标记；一个推送周期内的多次修改合并为一份最新完整状态。异步工作交给独立执行器，网络等待不持有直播或宿主锁。

| 模块 | 职责 |
| --- | --- |
| `live_events.rs` | 弹幕/SC 文本、单次收礼事件，不传连接、Cookie 或完整原始通知 |
| `extensions/mod.rs` | 领域接口、统一状态消息与异步任务类型 |
| `extensions/host.rs` | 内置注册、事件分发、版本、状态合并、只读查询和生命周期 |
| `extensions/storage.rs` | JSON 检查点、临时文件替换、损坏配置保护、跳过相同检查点 |
| `extensions/video_request.rs`、`voting.rs`、`overtime.rs` | 各扩展领域逻辑 |
| `extensions/tasks.rs`、`video_info.rs` | 异步视频抓取，最多 4 并发、15 秒请求超时 |
| `commands/extensions.rs` | 统一扩展 IPC，直接调用宿主 |
| `src/services/extensions.ts` | 桌面状态监听、初始快照、操作和查询 |
| `src/types/extensions.ts` | 扩展状态与请求类型，独立于直播数据类型 |

应用级循环每 100ms 分发桌面更新、每秒驱动扩展、每 5 秒保存检查点，独立于直播连接与扩展窗口。断开直播后投票仍可到期结束，手动操作仍会更新打开的扩展页面。收到投票弹幕时也检查截止时间，避免在下一次 tick 前接收过期选票。退出时断开直播、取消并等待后台任务，再保存最后检查点和关闭 OBS 服务。

加班机以 Rust `Instant` 单调时钟为准，不用 tick 次数累计；关闭窗口、调度延迟和多个浏览器不会加快或停止倒计时。重启读取检查点后暂停。点播任务仅按唯一请求 ID 回写；删除后再次点播不会接受过期回包。启动时恢复未完成抓取。抓取执行器持续接收新任务，空闲并发槽会及时补充，单个慢请求不会阻塞其他空闲槽；退出时取消在途请求。

## 桌面协议

三个扩展共用以下 IPC，不经过直播客户端和 `DataUpdate`：

| 命令 | 参数 / 结果 |
| --- | --- |
| `get_extension_snapshot` | `extensionId` → `{ extension_id, revision, state }` |
| `extension_request` | `extensionId`、`request` → 操作结果；同时更新状态版本 |
| `extension_query` | `extensionId`、`query` → 只读结果，不发状态事件、不落盘 |

状态事件名为 `extension-state:{id}`，内容与初始快照相同。版本号在当前应用运行期间单调递增。客户端先监听，再请求初始状态；仅接受更高版本，防止较慢查询覆盖新推送。关闭页面会取消监听并失效未完成的查询，重新打开时获取当前完整状态。操作完成后也主动刷新状态；若操作已生效但保存失败，仍刷新状态并保留错误提示。错误提示中的“刷新状态”会重新获取快照并复用已有监听，连续点击合并为一次查询；成功后清除提示，失败后可重新连接，不会重放可能已经生效的操作。

点播请求：`mark_watched`（`request_id`、`watched`）、`remove`（`request_id`）、`clear_watched`、`clear_all`。投票请求：`create`（`title`、`options`、`key_type`、`duration_secs`）、`end` / `delete`（`poll_id`）。投票只读查询：`voters`（`poll_id`、`option_key`）。投票公开状态不携带投票者列表或内部 UID 去重索引。

扩展窗口不初始化直播客户端。点播、投票页通过各自 store 连接统一客户端；加班机直接连接同一客户端。旧专用命令、旧扩展增量消息、直播快照中的扩展字段及兼容适配层已删除。

## 存储

所有扩展直接读写配置目录的 `extensions/{id}.json`，不包含 KV 外层结构或迁移分支。

| 文件 | 内容 |
| --- | --- |
| `extensions/video-request.json` | 点播请求数组；去重索引在恢复时重建 |
| `extensions/voting.json` | 完整投票数组，包含投票者与去重数据 |
| `extensions/overtime.json` | 加班机检查点 |
| `extensions/server.json` | 本地 HTTP 服务端口 |

原配置根目录的点播、投票文件不再读取，也不自动迁移。读入失败的新检查点不会被后台事件或定时保存覆盖；用户主动操作前备份为 `.json.invalid-*`，操作成功才允许写入。磁盘写入失败会明确提示“操作已生效，但扩展状态保存失败”。

## 新增扩展

1. 在 `src-tauri/src/extensions/` 实现 `Extension` 的固定 ID、`snapshot`、`checkpoint`、`restore`、`request`，在 `ExtensionHost::new` 注册。存储路径自动由 ID 生成，操作应先校验再修改。
2. 按需实现 `on_text`、`on_gift`、`tick`、`query`。异步工作通过 `take_effects` 与 `complete_effect` 交给任务层，不依赖直播连接或 UI。
3. 在 `src/types/extensions.ts` 声明状态/请求类型，在 `src/components/extension/registry.ts` 注册设置面板，通过 `createExtensionClient` 读取状态与执行操作，无需修改直播订阅枚举或添加专用 Tauri 命令。
4. 如需 OBS 展示，显式开启 `browser_visible`，提供独立 HTML / CSS / JS 并在 `extensions/server.rs` 静态资源表注册。默认扩展不可经 HTTP 读取；目前仅加班机对 OBS 开放。

目前为编译期注册的内置扩展，不执行外部下载的任意插件代码。新增事件继续使用最小化标准类型，扩展领域模块不构造桌面 `DataUpdate`。

## 只读 HTTP 协议

服务固定绑定 `127.0.0.1`，默认端口 `17654`。可在设置面板修改；冲突时明确报错，不自动变更已配置的 OBS 地址。资源使用 `include_bytes!` 嵌入可执行文件，开发和安装包均无需额外 Node 服务、Vite 进程或资源路径查找。

| 路径 | 内容 |
| --- | --- |
| `GET /api/extensions` | 协议版本 1 与允许浏览器读取的扩展 ID |
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
node --test scripts/test_extension_client.mjs scripts/test_overtime_overlay.mjs
```

Rust 测试覆盖单调计时、暂停、倍率、边界值、数量及规则过滤、重启恢复、连击去重到计时器的管线、HTTP 静态资源、SSE 重连和本机访问限制。随机规则测试覆盖边界、四种运算、数量、旧配置与具体抽签结果。Node 测试验证 OBS 入口的断线显示、快照校准、循环提示和队列切换、随机范围与结果展示、规则渲染和资源清理，不需要浏览器自动化。

扩展管线回归额外覆盖直接 JSON 检查点、待抓取任务恢复、删除后的过期回包、SC 金额与时间单位、并行投票与 UID 去重、断开后的到期检查、状态版本与合并、损坏文件保护，以及点播/投票不暴露给 HTTP。客户端测试覆盖查询/推送竞态、注册监听时关闭页面、重新连接、只读查询与保存失败后的状态刷新。

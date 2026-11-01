---
name: danmuji-release
description: 在 danmuji-next 工作区执行发版、发布新版本或准备发版日志。根据上次发版后的提交编写面向用户的简洁 UPDATE.md，再同步版本号、构建、生成并校验 Tauri updater 签名、上传腾讯云 COS。支持 patch/minor/major、0.0.1/0.1/1 和失败续跑；仅修改发版工具时不要执行真实发布。
---

# danmuji-next 发版

在仓库根目录运行所有命令。脚本位置是 `scripts/release.py`，与本 skill 一同维护。
用户要求「执行发版」时，完成日志准备和真实 COS 发布；这已经包含上传授权，不重复索取确认。
用户只要求准备、预演或创建/修改发版工具时，完成对应范围即可。

## 凭据约束

- 不读取任何 `launch.json`，也不读取、打印或搜索 `.env` 的值。查代码或提交时明确指定安全文件，避免无范围的 `git show` / `git diff`。
- 运行时由脚本加载仓库根目录 `.env`，已有进程环境变量优先；不要把 key 拼到命令行、代码、日志或回复中。
- 所需变量：`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（可为空）、`COS_SECRET_ID`、`COS_SECRET_KEY`、`COS_BUCKET`、`COS_REGION`。`COS_CDN_DOMAIN` 可选。
- 缺失时仅报告变量名，不要求用户在聊天中粘贴密钥。不生成替代签名密钥，不修改 updater 公钥。

## 1. 检查状态和选择版本

1. 查看 `git status --short`；已有 `.release/state.json` 时检查阶段。如果是未完成发版，按下方「续跑」处理，不重新 bump。
2. 默认 `patch`（+0.0.1）；用户指定 `minor` / `0.1` 时升级次版本并清零 patch，指定 `major` / `1` 时升级主版本并清零其余部分。
3. 执行 `python scripts/release.py plan patch`（按需替换升级类型）。它只读取 Git 和版本文件，不读取 `.env`，输出 `.release/plan.json` 和 `.release/commits.md`。
4. 检查输出的基线来源和全部提交。顺序为本地成功发版记录 → 当前版本的精确 tag → 从三个版本文件历史推断当前版本的起点。历史 tag 可能落后很多，不能直接用最近 tag。基线推断与已知发布记录不符时，用 `plan <类型> --since <tag或SHA>`；无可靠基线时才向用户询问实际发布提交。
5. 没有新提交则说明情况，不制造新版本。除本次 `UPDATE.md` 外的源码改动必须已提交；不要擅自丢弃或提交用户工作。如果用户另有提交授权，按授权处理后重新 plan。

## 2. AI 编写本次 UPDATE.md

读取 `.release/commits.md` 的完整提交消息和涉及文件。对实际功能变化不明确的提交，
查看相应安全文件的 diff，例如 `git show <SHA> -- src/components/具体文件.vue`。
检查全部提交，识别撤销或后续修复，再概括最终改动为用户带来的变化。
完整提交消息、SHA、涉及文件和基线仅保留在 `.release/commits.md`、`.release/plan.json` 等内部记录中。

`UPDATE.md` 只保留本次发版内容；旧的已发布日志保存在 `.release/<版本>/UPDATE.md` 和
COS 的 `releases/<版本>.json`。保留现有日志中仍适用的用户表述，纠正过时项。

写作要求：

- 面向普通用户，用少量中文短句说明新增能力、解决的问题或使用体验的改善；每条一个重点，合并同一功能的多次改动。
- 不逐条翻译 commit，不列提交标题、SHA、源码路径、内部实现术语或发版元数据（包括隐藏注释）。OBS、许愿机等用户熟悉的产品名称可保留。
- 没有用户可感知影响的测试、重构和维护改动不必单独列出。不把内部重构自动描述成性能或稳定性提升，不编造未经改动支持的收益。
- 保持简洁，通常使用版本标题加几条要点即可；无需固定分类或凑条数。

版本必须取自实际计划，示例：

```markdown
# 2.7.4 更新

- 支持显示跨房弹幕，可在设置中自由开关。
- 优化弹幕显示，改善弹幕较多时的流畅度。
- 修复许愿机部分目标无法保存的问题。
```

不要写 TODO、待补充或未经 diff 支持的功能。不要写入凭据和本机私人路径。
重新生成计划后，重新核对日志是否反映了该计划的最终改动。

## 3. 脚本发版

首次运行且缺少依赖时：

```bash
pnpm install --frozen-lockfile
python -m pip install -r scripts/requirements-release.txt
```

运行：

```bash
python scripts/release.py run --dry-run
python scripts/release.py run
```

预演会验证三个版本文件及锁文件、日志版本与内容、HEAD 和工作区状态，不读取 `.env`、改版本、构建或联网。
正式执行前，脚本检查 Windows x64 工具链、配置、环境变量与远端版本，然后依次：

1. 同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`，以及 `Cargo.lock` 中本项目版本。
2. 运行 `pnpm run tauri:build`，在子进程环境注入签名变量；Tauri 生成安装包及 `.exe.sig`。
3. 选择精确版本的 x64 NSIS 安装包、相邻签名和便携版，用 updater 公钥验证签名；复制到 `.release/<版本>/` 并记录 SHA-256。
4. 上传并校验产物、版本清单，最后发布 `update.json` 并回读验证；记录成功版本与源 HEAD，供下次选择基线。

这里的 sign 是 Tauri updater 签名。当前流程没有 Windows Authenticode 证书签名配置，不能把更新签名描述为消除 SmartScreen 提示。

只构建到本地时使用 `run --no-upload`，之后可用 `run --resume` 发布同一组产物。
不要绕过主入口直接调用旧版 bump 脚本或上传器来完成普通发版。

## 失败续跑

- 构建或上传失败：检查脚本报告，然后运行 `python scripts/release.py run --resume`。版本只升级一次；构建已成功时只校验并上传暂存产物。
- 不读取原始凭据来排错。不盲目循环重试；同一错误持续出现时先处理工具链、配置、网络或产物问题。
- `HEAD`、日志或产物变更时脚本会拒绝续跑。修复需要改源码且阶段仍为 `prepared` / `built` 时，使用 `python scripts/release.py reset`，它保留故障记录并恢复原版本；修复并提交源码后重新 plan。`uploading` / `published` 阶段禁止 reset，必须检查远端并处理同一版本。不要通过删除状态来跳过冲突检查。
- 强制终止造成遗留锁时，确认没有发版进程再移除 `.release/active.lock`。
- 脚本不自动 commit、打 tag 或 push。只有用户另外授权时才执行这些 Git 写入，不把其他工作一起提交。

## 完成回复

报告旧版本 → 新版本、基线与提交数、更新摘要、构建和签名校验结果、COS 发布结果，以及
`.release/<版本>/` 的本地产物位置。预演或失败不能报告为已发布；失败时说明停在哪个阶段及续跑命令。

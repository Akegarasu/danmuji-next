# 发布清单

1. 确认 `blivedm-rs` 仍是可用的 crates.io 包名。
2. 由项目所有者选择许可证，在仓库中加入 `LICENSE`，并在 `Cargo.toml` 设置匹配的 `license` 或 `license-file`。
3. 删除 `Cargo.toml` 中的 `publish = false`。
4. 按语义化版本更新 `version` 和变更记录。
5. 运行：

   ```bash
   cargo fmt --check --manifest-path crates/blivedm/Cargo.toml
   cargo test --manifest-path crates/blivedm/Cargo.toml
   cargo package --manifest-path crates/blivedm/Cargo.toml
   cargo publish --dry-run --manifest-path crates/blivedm/Cargo.toml
   ```

6. 检查生成包中的文件和 docs.rs 文档后，再执行 `cargo publish`。

crates.io 上的 `blivedm` 已被其他项目占用，所以本项目使用包名 `blivedm-rs`；`[lib] name = "blivedm"` 保证 Rust 代码中的导入名仍为 `blivedm`。

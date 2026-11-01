#!/usr/bin/env python3
"""同步应用版本；保留原有仅升级版本 / 本地构建入口。完整发版使用 release.py。"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import sys
from pathlib import Path

from release_support import ROOT, ReleaseError, configure_console, load_release_env, require_env, run_command


PACKAGE_NAME = "danmuji-next"
VERSION_PATTERN = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
PARTS = {"patch": "patch", "0.0.1": "patch", "minor": "minor", "0.1": "minor",
         "0.1.0": "minor", "major": "major", "1": "major", "1.0.0": "major"}
VERSION_FILES = ("package.json", "src-tauri/Cargo.toml", "src-tauri/tauri.conf.json")
LOCK_FILE = "src-tauri/Cargo.lock"


def ensure_semver(version: str) -> tuple[int, int, int]:
    match = VERSION_PATTERN.fullmatch(version)
    if not match:
        raise ValueError(f"不支持的版本号：{version}")
    return tuple(int(group) for group in match.groups())


def bump_version(version: str, part: str) -> str:
    major, minor, patch = ensure_semver(version)
    part = PARTS[part]
    if part == "major":
        return f"{major + 1}.0.0"
    if part == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def package_block(text: str, lock: bool = False) -> re.Match:
    pattern = r"(?ms)^\[\[package\]\][^\[]*" if lock else r"(?ms)^\[package\][^\[]*"
    for block in re.finditer(pattern, text):
        if not lock or re.search(r'^name\s*=\s*"danmuji-next"\s*$', block[0], re.M):
            return block
    raise ValueError("找不到 danmuji-next 的 Cargo package 段")


def version_from_text(path: str, text: str) -> str:
    if path.endswith(".json"):
        return json.loads(text)["version"]
    block = package_block(text, lock=path.endswith(".lock"))
    match = re.search(r'^version\s*=\s*"([^"]+)"', block[0], re.M)
    if not match:
        raise ValueError(f"找不到版本号：{path}")
    return match[1]


def get_current_version(root: Path = ROOT) -> str:
    versions = {name: version_from_text(name, (root / name).read_text(encoding="utf-8"))
                for name in VERSION_FILES}
    if len(set(versions.values())) != 1:
        raise ValueError("三个版本文件不一致：" + ", ".join(f"{k}={v}" for k, v in versions.items()))
    version = versions[VERSION_FILES[0]]
    ensure_semver(version)
    return version


def version_updates(root: Path, old_version: str, new_version: str) -> dict[Path, bytes]:
    """先校验所有文件并生成补丁，避免中途发现不一致后只更新部分文件。"""
    ensure_semver(new_version)
    names = list(VERSION_FILES)
    if (root / LOCK_FILE).is_file():
        names.append(LOCK_FILE)
    result = {}
    for name in names:
        path = root / name
        text = path.read_bytes().decode("utf-8")
        if version_from_text(name, text) != old_version:
            raise ValueError(f"版本不一致：{name}，预期 {old_version}")
        if name.endswith(".json"):
            updated, count = re.subn(r'("version"\s*:\s*")' + re.escape(old_version) + r'(")',
                                     lambda m: m[1] + new_version + m[2], text, count=1)
            expected = json.loads(text)
            expected["version"] = new_version
            if count != 1 or json.loads(updated) != expected:
                raise ValueError(f"无法准确定位顶层 version：{name}")
        else:
            block = package_block(text, lock=name.endswith(".lock"))
            new_block, count = re.subn(r'(?m)^(version\s*=\s*")[^"]+("[^\r\n]*)',
                                       lambda m: m[1] + new_version + m[2], block[0], count=1)
            if count != 1:
                raise ValueError(f"无法更新版本：{name}")
            updated = text[:block.start()] + new_block + text[block.end():]
        result[path] = updated.encode("utf-8")
    return result


def update_versions(old_version: str, new_version: str, root: Path = ROOT) -> None:
    updates = version_updates(root, old_version, new_version)
    originals = {path: path.read_bytes() for path in updates}
    written = []
    try:
        for path, data in updates.items():
            written.append(path)
            path.write_bytes(data)
    except OSError:
        for path in written:
            path.write_bytes(originals[path])
        raise


def run_tauri_build(root: Path = ROOT, env: dict[str, str] | None = None) -> None:
    env = env if env is not None else load_release_env(root)
    require_env(env, ("TAURI_SIGNING_PRIVATE_KEY",))
    pnpm = shutil.which("pnpm", path=env.get("PATH"))
    if not pnpm:
        raise ReleaseError("未找到 pnpm")
    run_command([pnpm, "run", "tauri:build"], root, env)


def main() -> int:
    configure_console()
    parser = argparse.ArgumentParser(description="同步版本号，并可选执行本地构建与签名")
    parser.add_argument("part", nargs="?", choices=PARTS, default="patch")
    parser.add_argument("--no-build", action="store_true", help="仅升级版本")
    parser.add_argument("--dry-run", action="store_true", help="只预览，不读取 .env、不改文件")
    args = parser.parse_args()
    try:
        current = get_current_version()
        target = bump_version(current, args.part)
        version_updates(ROOT, current, target)
        print(f"版本：{current} -> {target}")
        if args.dry_run:
            return 0
        env = None
        if not args.no_build:
            env = load_release_env()
            require_env(env, ("TAURI_SIGNING_PRIVATE_KEY",))
        update_versions(current, target)
        print("三个版本文件及 Cargo.lock 已同步。")
        if not args.no_build:
            run_tauri_build(env=env)
        return 0
    except (ValueError, ReleaseError) as error:
        print(f"错误：{error}", file=sys.stderr)
        return 1
    except OSError:
        print("错误：文件或命令访问失败", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

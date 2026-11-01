#!/usr/bin/env python3
"""AI 准备 UPDATE.md 后执行的发版入口：plan / run / run --resume。"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import time
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace

from bump_version_and_build import (LOCK_FILE, PARTS, VERSION_FILES, bump_version,
                                    get_current_version, run_tauri_build, version_from_text, version_updates)
from cos_upload import (COS_ENV_NAMES, artifact_keys, check_remote_version, create_client,
                        generate_update_json, public_base_url, publish_release,
                        remote_manifest, resolve_artifacts)
from release_support import (ROOT, ReleaseError, configure_console, load_release_env, require_env,
                             sha256_file, verify_updater_signature, write_json)


STATE_DIR = ".release"
MANAGED_FILES = (*VERSION_FILES, LOCK_FILE, "UPDATE.md")


def git(root: Path, *args: str, optional: bool = False) -> str:
    result = subprocess.run(["git", *args], cwd=root, capture_output=True,
                            text=True, encoding="utf-8", errors="replace")
    if result.returncode:
        if optional:
            return ""
        raise ReleaseError("Git 操作失败，请检查仓库状态和指定的提交引用")
    return result.stdout.rstrip("\n")


def resolve_ref(root: Path, ref: str) -> str:
    return git(root, "rev-parse", "--verify", "--end-of-options", ref + "^{commit}")


def ancestor(root: Path, commit: str, head: str) -> bool:
    return subprocess.run(["git", "merge-base", "--is-ancestor", commit, head], cwd=root,
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0


def release_base(root: Path, version: str, head: str, since: str | None) -> tuple[str, str]:
    if since:
        base, source = resolve_ref(root, since), "用户指定的 --since"
    else:
        record_path = root / STATE_DIR / "last-release.json"
        record = json.loads(record_path.read_text(encoding="utf-8")) if record_path.exists() else {}
        if record.get("version") == version:
            base, source = resolve_ref(root, record["head"]), "本地成功发版记录"
        else:
            base, source = "", ""
            for tag in ("v" + version, version):
                candidate = git(root, "rev-parse", "--verify", "refs/tags/" + tag + "^{commit}", optional=True)
                if candidate and ancestor(root, candidate, head):
                    base, source = candidate, f"版本标签 {tag}"
                    break
            if not base:
                # 老流程没有逐次打 tag：找当前版本首次在三个清单中一致出现的提交。
                history = git(root, "log", "--first-parent", "--format=%H", head, "--", *VERSION_FILES)
                for commit in history.splitlines():
                    versions = []
                    for name in VERSION_FILES:
                        content = git(root, "show", f"{commit}:{name}", optional=True)
                        if not content:
                            continue
                        versions.append(version_from_text(name, content))
                    if versions == [version] * len(VERSION_FILES):
                        base = commit
                    elif base:
                        break
                source = "从三个版本文件的历史推断（未找到当前版本 tag）"
            if not base:
                raise ReleaseError("无法确定上次发版提交，请用 plan --since <tag或SHA> 指定")
    if not ancestor(root, base, head):
        raise ReleaseError("发版基线不是当前 HEAD 的祖先，请检查 --since 或本地记录")
    return base, source


def changed_paths(root: Path) -> set[str]:
    # 只查询文件名，不读取任何 diff、.env 或 launch.json 内容。
    tracked = git(root, "diff", "--name-only", "-z", "HEAD", "--").split("\0")
    untracked = git(root, "ls-files", "--others", "--exclude-standard", "-z").split("\0")
    return {p for p in (*tracked, *untracked) if p and not p.startswith(STATE_DIR + "/")}


def check_worktree(root: Path, resume: bool) -> None:
    allowed = set(MANAGED_FILES) if resume else {"UPDATE.md"}
    unexpected = changed_paths(root) - allowed
    if unexpected:
        raise ReleaseError("以下更改未包含在提交记录中，请先处理并重新 plan：" + ", ".join(sorted(unexpected)))


def create_plan(root: Path, part: str = "patch", since: str | None = None) -> dict:
    version = get_current_version(root)
    target = bump_version(version, part)
    version_updates(root, version, target)
    head = resolve_ref(root, "HEAD")
    base, source = release_base(root, version, head, since)
    commits = []
    for sha in git(root, "rev-list", "--reverse", f"{base}..{head}").splitlines():
        message = git(root, "show", "--no-patch", "--format=%B", sha)
        files = git(root, "diff-tree", "--root", "--no-commit-id", "--name-only", "-r", "-m", sha)
        commits.append({"sha": sha, "subject": message.splitlines()[0], "message": message,
                        "files": sorted(set(files.splitlines()))})
    return {"schema": 1, "current_version": version, "version": target, "part": PARTS[part],
            "base": base, "base_source": source, "head": head, "commits": commits}


def show_plan(plan: dict) -> str:
    lines = [f"# 发版计划 {plan['current_version']} -> {plan['version']}",
             f"基线：{plan['base']}（{plan['base_source']}）", f"HEAD：{plan['head']}",
             f"提交数：{len(plan['commits'])}", ""]
    for commit in plan["commits"]:
        lines += [f"## {commit['sha'][:8]} {commit['subject']}", "", commit["message"], "",
                  "涉及文件：" + ", ".join(commit["files"]), ""]
    return "\n".join(lines)


def validate_notes(root: Path, plan: dict) -> str:
    path = root / "UPDATE.md"
    if not path.is_file():
        raise ReleaseError("请先让 agent 根据发版计划编写 UPDATE.md")
    notes = path.read_text(encoding="utf-8").strip()
    if not re.match(r"^#\s+" + re.escape(plan["version"]) + r"\s+更新\s*(?:\n|$)", notes):
        raise ReleaseError(f"UPDATE.md 必须以 # {plan['version']} 更新 开头")
    # 用户日志只需版本标题与实际更新说明；提交范围由内部计划和发版状态保存。
    summary = notes.partition("\n")[2]
    summary = re.sub(r"<!--.*?-->", "", summary, flags=re.S)
    summary = re.sub(r"(?m)^#{1,6}\s.*$", "", summary).strip(" \t\r\n-* ")
    if not summary or re.search(r"TODO|待补充|待填写", summary, re.I):
        raise ReleaseError("UPDATE.md 的更新内容尚未完成")
    return notes


@contextmanager
def release_lock(root: Path):
    path = root / STATE_DIR / "active.lock"
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        handle = path.open("x", encoding="utf-8")
    except FileExistsError:
        raise ReleaseError("已有发版锁 .release/active.lock；确认没有发版进程后才可移除遗留锁") from None
    try:
        with handle:
            handle.write(str(os.getpid()))
        yield
    finally:
        path.unlink(missing_ok=True)


def preflight(root: Path, env: dict[str, str], upload: bool) -> None:
    if os.name != "nt":
        raise ReleaseError("此流水线发布 Windows x64 产物，请在 Windows 上执行")
    require_env(env, ("TAURI_SIGNING_PRIVATE_KEY",) + (COS_ENV_NAMES if upload else ()))
    for module in ("cryptography", "qcloud_cos") if upload else ("cryptography",):
        if importlib.util.find_spec(module) is None:
            raise ReleaseError("缺少发版依赖，请运行 python -m pip install -r scripts/requirements-release.txt")
    for executable in ("pnpm", "cargo", "rustc"):
        if not shutil.which(executable, path=env.get("PATH")):
            raise ReleaseError(f"未找到 {executable}")
    if not (root / "node_modules/@tauri-apps/cli").is_dir():
        raise ReleaseError("请先运行 pnpm install --frozen-lockfile")
    for name in ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "TAURI_CONFIG"):
        if env.get(name):
            raise ReleaseError(f"请移除 {name} 覆盖配置，以使用本流水线固定的 Windows x64 产物路径")
    rust = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, env=env, check=True)
    if "host: x86_64-pc-windows-msvc" not in rust.stdout:
        raise ReleaseError("需要 x86_64-pc-windows-msvc Rust 工具链")
    config = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    if config.get("bundle", {}).get("createUpdaterArtifacts") is not True:
        raise ReleaseError("bundle.createUpdaterArtifacts 必须为 true，才能生成 Tauri v2 .exe.sig")
    targets = config["bundle"].get("targets", "all")
    if targets != "all" and "nsis" not in targets:
        raise ReleaseError("bundle.targets 必须包含 nsis")
    if not config.get("plugins", {}).get("updater", {}).get("pubkey"):
        raise ReleaseError("缺少 updater.pubkey")


def apply_prepared_versions(root: Path, state: dict) -> None:
    # 中断可能发生在多个文件写入之间；仅接受本次记录中的原文或目标文本。
    for name, expected in state["version_after"].items():
        actual = (root / name).read_bytes().decode("utf-8")
        if actual not in (state["version_before"][name], expected):
            raise ReleaseError(f"发版途中 {name} 被修改，已停止续跑")
    for name, expected in state["version_after"].items():
        (root / name).write_bytes(expected.encode("utf-8"))


def verify_state_inputs(root: Path, state: dict) -> None:
    if resolve_ref(root, "HEAD") != state["plan"]["head"]:
        raise ReleaseError("HEAD 已改变；请先处理本次未完成发版，不能混用新提交和旧产物")
    notes = validate_notes(root, state["plan"])
    if hashlib.sha256(notes.encode("utf-8")).hexdigest() != state["notes_sha256"]:
        raise ReleaseError("UPDATE.md 在发版开始后被修改，不能直接续跑旧产物")
    check_worktree(root, resume=True)


def verify_staged(root: Path, state: dict) -> tuple[dict, tuple[Path, Path, Path]]:
    directory = root / STATE_DIR / state["plan"]["version"]
    for name, expected in state["hashes"].items():
        if not (directory / name).is_file() or sha256_file(directory / name) != expected:
            raise ReleaseError(f"暂存产物被修改或丢失：{name}")
    artifacts = tuple(directory / key for key in artifact_keys(state["plan"]["version"]))
    config = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    verify_updater_signature(artifacts[0], artifacts[1], config["plugins"]["updater"]["pubkey"])
    return json.loads((directory / "update.json").read_text(encoding="utf-8")), artifacts


def reset_prepublication(root: Path) -> None:
    """仅撤销从未开始上传的本地准备，保留日志和故障记录。"""
    path = root / STATE_DIR / "state.json"
    if not path.is_file():
        raise ReleaseError("没有需要重置的本地发版")
    state = json.loads(path.read_text(encoding="utf-8"))
    if state["phase"] not in ("prepared", "built"):
        raise ReleaseError("已经开始上传或已发布，不能重置并复用版本号；请检查远端状态并续跑")
    for name, original in state["version_before"].items():
        if (root / name).read_bytes().decode("utf-8") not in (original, state["version_after"][name]):
            raise ReleaseError(f"{name} 有额外修改，无法自动恢复版本；请先保留并处理这些修改")
    archive = root / STATE_DIR / "abandoned" / (state["plan"]["version"] + f"-{time.time_ns()}.json")
    write_json(archive, state)
    for name, original in state["version_before"].items():
        (root / name).write_bytes(original.encode("utf-8"))
    path.unlink()
    (root / STATE_DIR / "plan.json").unlink(missing_ok=True)
    print("本地版本已恢复；UPDATE.md、产物和故障记录已保留。修复并提交源码后重新 plan。")


def execute(root: Path, args) -> None:
    state_path = root / STATE_DIR / "state.json"
    previous = json.loads(state_path.read_text(encoding="utf-8")) if state_path.exists() else None
    if args.resume:
        if not previous:
            raise ReleaseError("没有可续跑的发版")
        state, plan = previous, previous["plan"]
        if args.part and PARTS[args.part] != plan["part"]:
            raise ReleaseError("续跑不能改变升级类型")
        verify_state_inputs(root, state)
    else:
        if previous and previous["phase"] != "published":
            raise ReleaseError("上次发版未完成，请使用 run --resume，避免重复升级版本")
        plan_path = root / STATE_DIR / "plan.json"
        if not plan_path.is_file():
            raise ReleaseError("请先运行 python scripts/release.py plan，然后由 agent 编写 UPDATE.md")
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        if args.part and PARTS[args.part] != plan["part"]:
            raise ReleaseError("升级类型与已有计划不同，请重新 plan 并编写日志")
        if get_current_version(root) != plan["current_version"] or resolve_ref(root, "HEAD") != plan["head"]:
            raise ReleaseError("版本或 HEAD 已改变，请重新 plan 并生成 UPDATE.md")
        if not plan["commits"]:
            raise ReleaseError("上次发版后没有新提交，无需发版")
        notes = validate_notes(root, plan)
        check_worktree(root, resume=False)
        updates = version_updates(root, plan["current_version"], plan["version"])
        state = {"phase": "prepared", "plan": plan,
                 "notes_sha256": hashlib.sha256(notes.encode("utf-8")).hexdigest(),
                 "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                 "version_before": {str(p.relative_to(root)).replace("\\", "/"): p.read_bytes().decode("utf-8") for p in updates},
                 "version_after": {str(p.relative_to(root)).replace("\\", "/"): data.decode("utf-8") for p, data in updates.items()}}
    if args.dry_run:
        print(f"预演通过：{plan['current_version']} -> {plan['version']}，{len(plan['commits'])} 个提交。")
        print("将同步版本、构建并签名、校验产物" + ("。" if args.no_upload else "、上传 COS，最后发布 update.json。"))
        print("未读取 .env，未修改版本，未构建或联网；运行时依赖和凭据在正式执行前检查。")
        return
    if state["phase"] == "published":
        print(f"{plan['version']} 已发布，无需重复执行。")
        return
    env = load_release_env(root)
    preflight(root, env, upload=not args.no_upload)
    base_url = public_base_url(env.get("COS_CDN_DOMAIN", "updater.anzu.link"))
    client = create_client(env) if not args.no_upload else None
    if client is not None and state["phase"] == "prepared":
        # 构建前就拒绝已经发布过的版本，避免长时间构建后才发现冲突。
        check_remote_version(remote_manifest(client, env["COS_BUCKET"]), {"version": plan["version"]})
    if not args.resume:
        write_json(state_path, state)
    apply_prepared_versions(root, state)
    if state["phase"] == "prepared":
        print(f"[1/3] 版本已同步为 {plan['version']}，开始构建与签名……", flush=True)
        started = time.time()
        run_tauri_build(root, env)
        artifacts = resolve_artifacts(SimpleNamespace(version=plan["version"], nsis_zip=None, sig_file=None,
                                                      portable_exe=None, artifacts_dir=root / "src-tauri/target/release"))
        if any(path.stat().st_mtime < started - 2 for path in artifacts[:2]):
            raise ReleaseError("安装包或签名不是本次构建生成，已停止上传")
        config = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
        signature = verify_updater_signature(artifacts[0], artifacts[1], config["plugins"]["updater"]["pubkey"])
        verify_state_inputs(root, state)
        # 构建不能偷偷改变版本清单或锁文件。
        for name, expected in state["version_after"].items():
            if (root / name).read_bytes().decode("utf-8") != expected:
                raise ReleaseError(f"构建修改了 {name}，请检查依赖或构建配置后再发布")
        directory = root / STATE_DIR / plan["version"]
        directory.mkdir(parents=True, exist_ok=True)
        keys = artifact_keys(plan["version"])
        for key, source in zip(keys, artifacts):
            shutil.copy2(source, directory / key)
        notes = validate_notes(root, plan)
        manifest = generate_update_json(plan["version"], signature, f"{base_url}/{keys[0]}",
                                        f"{base_url}/{keys[2]}", notes, state["pub_date"])
        write_json(directory / "update.json", manifest)
        (directory / "UPDATE.md").write_text(notes + "\n", encoding="utf-8")
        state["hashes"] = {name: sha256_file(directory / name) for name in (*keys, "update.json", "UPDATE.md")}
        state["phase"] = "built"
        write_json(state_path, state)
    manifest, staged = verify_staged(root, state)
    print("[2/3] 安装包签名及暂存文件 SHA-256 校验通过。")
    if args.no_upload:
        print("本地构建完成。发布这些产物：python scripts/release.py run --resume")
        return
    verify_state_inputs(root, state)
    print("[3/3] 上传 COS，最后切换更新清单……", flush=True)
    state["phase"] = "uploading"
    write_json(state_path, state)
    publish_release(client, env["COS_BUCKET"], manifest, staged)
    record = {"version": plan["version"], "head": plan["head"], "base": plan["base"],
              "published_at": datetime.now(timezone.utc).isoformat(), "hashes": state["hashes"]}
    write_json(root / STATE_DIR / "last-release.json", record)
    state["phase"] = "published"
    write_json(state_path, state)
    print(f"发布完成：{plan['version']}，清单与产物保存在 .release/{plan['version']}/")


def main(argv: list[str] | None = None, root: Path = ROOT) -> int:
    configure_console()
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    plan_parser = commands.add_parser("plan", help="列出距上次发版的提交，供 AI 编写 UPDATE.md")
    plan_parser.add_argument("part", nargs="?", choices=PARTS, default="patch")
    plan_parser.add_argument("--since", help="覆盖上次发版基线（tag 或 commit）")
    plan_parser.add_argument("--dry-run", action="store_true", help="只输出计划，不写任何文件")
    run_parser = commands.add_parser("run", help="使用已有计划和 UPDATE.md 发版")
    run_parser.add_argument("part", nargs="?", choices=PARTS)
    run_parser.add_argument("--resume", action="store_true", help="从失败阶段续跑，不再次升级版本")
    run_parser.add_argument("--no-upload", action="store_true", help="完成本地构建、签名和校验后停止")
    run_parser.add_argument("--dry-run", action="store_true", help="校验计划与日志，不读 .env、不改版本、不构建、不联网")
    commands.add_parser("reset", help="仅在尚未开始上传时恢复原版本并保留故障记录，供修复源码后重新规划")
    args = parser.parse_args(argv)
    try:
        if args.command == "plan":
            plan = create_plan(root, args.part, args.since)
            report = show_plan(plan)
            if not args.dry_run:
                with release_lock(root):
                    state_path = root / STATE_DIR / "state.json"
                    if state_path.exists() and json.loads(state_path.read_text(encoding="utf-8"))["phase"] != "published":
                        raise ReleaseError("已有未完成发版，请先 run --resume；不能重新升级版本")
                    write_json(root / STATE_DIR / "plan.json", plan)
                    (root / STATE_DIR / "commits.md").write_text(report + "\n", encoding="utf-8")
            print(report)
        elif args.command == "reset":
            with release_lock(root):
                reset_prepublication(root)
        elif args.dry_run:
            execute(root, args)
        else:
            with release_lock(root):
                execute(root, args)
        return 0
    except (ReleaseError, ValueError) as error:
        print(f"错误：{error}", file=sys.stderr)
        return 1
    except (OSError, KeyError, subprocess.SubprocessError):
        print("错误：文件、配置或工具访问失败；未自动回滚已准备的版本，可检查后使用 --resume", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("发版已中断；下一次使用 run --resume。", file=sys.stderr)
        return 130


if __name__ == "__main__":
    raise SystemExit(main())

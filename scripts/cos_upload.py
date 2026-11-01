#!/usr/bin/env python3
"""上传 Windows x64 安装包、签名、便携版，最后切换 COS 的 update.json。"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit

from bump_version_and_build import ensure_semver
from release_support import (ROOT, ReleaseError, configure_console, load_release_env, require_env,
                             sha256_file, verify_updater_signature, write_json)


COS_ENV_NAMES = ("COS_SECRET_ID", "COS_SECRET_KEY", "COS_BUCKET", "COS_REGION")


def resolve_artifacts(args) -> tuple[Path, Path, Path]:
    version = args.version.removeprefix("v")
    ensure_semver(version)
    explicit = (args.nsis_zip, args.sig_file, args.portable_exe)
    if any(explicit):
        if not all(explicit):
            raise ReleaseError("请同时指定 --installer、--sig-file、--portable-exe")
        installer, signature, portable = map(Path, explicit)
    elif args.artifacts_dir:
        directory = Path(args.artifacts_dir)
        name = f"danmuji-next_{version}_x64-setup.exe"
        candidates = {p.resolve() for p in (directory / name, directory / "bundle/nsis" / name)
                      if p.is_file()}
        if len(candidates) != 1:
            raise ReleaseError(f"需要唯一的本版本 x64 安装包：{name}，找到 {len(candidates)} 个")
        installer = candidates.pop()
        signature = installer.with_name(installer.name + ".sig")
        portable_candidates = [directory / f"danmuji-next_{version}_windows_x64_portable.exe",
                               directory / "danmuji-next.exe"]
        portable = next((p for p in portable_candidates if p.is_file()), portable_candidates[-1])
    else:
        raise ReleaseError("请指定 --artifacts-dir 或三个产物的完整路径")
    if installer.name != f"danmuji-next_{version}_x64-setup.exe":
        raise ReleaseError("安装包名称必须与本次版本和 Windows x64 架构匹配")
    if signature.resolve() != installer.with_name(installer.name + ".sig").resolve():
        raise ReleaseError("签名文件必须是所选安装包同目录下的 .exe.sig")
    for path in (installer, signature, portable):
        if not path.is_file() or path.stat().st_size == 0:
            raise ReleaseError(f"产物不存在或为空：{path.name}")
    return installer.resolve(), signature.resolve(), portable.resolve()


def public_base_url(domain: str) -> str:
    url = domain if "://" in domain else "https://" + domain
    parsed = urlsplit(url)
    if (parsed.scheme != "https" or not parsed.netloc or parsed.username or parsed.password
            or parsed.query or parsed.fragment or parsed.path not in ("", "/")):
        raise ReleaseError("COS_CDN_DOMAIN 必须是 HTTPS 域名，不能含路径、凭据或查询参数")
    return url.rstrip("/")


def generate_update_json(version: str, signature: str, nsis_url: str, portable_url: str,
                         notes: str = "", pub_date: str | None = None) -> dict:
    return {
        "version": version,
        "notes": notes,
        "pub_date": pub_date or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {"windows-x86_64": {"signature": signature, "url": nsis_url}},
        "portable_url": portable_url,
    }


def artifact_keys(version: str) -> tuple[str, str, str]:
    installer = f"danmuji-next_{version}_x64-setup.exe"
    return installer, installer + ".sig", f"danmuji-next_{version}_windows_x64_portable.exe"


def create_client(env: dict[str, str]):
    require_env(env, COS_ENV_NAMES)
    try:
        from qcloud_cos import CosConfig, CosS3Client
    except ImportError:
        raise ReleaseError("请先安装 scripts/requirements-release.txt 中的 COS SDK") from None
    return CosS3Client(CosConfig(Region=env["COS_REGION"], SecretId=env["COS_SECRET_ID"],
                                 SecretKey=env["COS_SECRET_KEY"], Scheme="https"))


def missing_object(error: Exception) -> bool:
    return callable(getattr(error, "get_status_code", None)) and str(error.get_status_code()) == "404"


def remote_manifest(client, bucket: str) -> dict | None:
    try:
        response = client.get_object(Bucket=bucket, Key="update.json")
    except Exception as error:
        if missing_object(error):
            return None
        raise ReleaseError("无法读取 COS 当前更新清单（远端错误详情已隐藏）") from None
    try:
        stream = response["Body"].get_raw_stream()
        try:
            return json.loads(stream.read())
        finally:
            stream.close()
    except Exception:
        raise ReleaseError("COS 当前 update.json 无效，已停止发布") from None


def check_remote_version(current: dict | None, manifest: dict) -> None:
    if current is None:
        return
    try:
        remote_version = ensure_semver(current["version"].removeprefix("v"))
    except (ValueError, KeyError, TypeError, AttributeError):
        raise ReleaseError("COS 当前版本号无效，已停止发布") from None
    target = ensure_semver(manifest["version"])
    if remote_version > target:
        raise ReleaseError("COS 已有更高版本，禁止回退更新清单")
    if remote_version == target and current != manifest:
        raise ReleaseError("COS 已发布同版本的不同清单，禁止覆盖；请使用新版本号")


def upload_immutable(client, bucket: str, key: str, source: Path | bytes) -> None:
    digest = sha256_file(source) if isinstance(source, Path) else hashlib.sha256(source).hexdigest()
    size = source.stat().st_size if isinstance(source, Path) else len(source)

    def matches(metadata: dict) -> bool:
        headers = {k.lower(): str(v) for k, v in metadata.items()}
        return headers.get("x-cos-meta-sha256") == digest and headers.get("content-length") == str(size)

    try:
        existing = client.head_object(Bucket=bucket, Key=key)
    except Exception as error:
        if not missing_object(error):
            raise
        existing = None
    if existing is not None:
        if not matches(existing):
            raise ReleaseError(f"COS 同名对象与本次内容不同，禁止覆盖：{key}")
        print(f"  已校验，跳过：{key}")
        return
    # COS Python SDK 的 Metadata 字典会原样成为 HTTP 头，必须保留 x-cos-meta- 前缀。
    common = dict(Bucket=bucket, Key=key, Metadata={"x-cos-meta-sha256": digest},
                  CacheControl="public, max-age=31536000, immutable")
    if isinstance(source, Path):
        client.upload_file(LocalFilePath=str(source), EnableMD5=True, **common)
    else:
        client.put_object(Body=source, ContentType="application/json; charset=utf-8",
                          EnableMD5=True, **common)
    if not matches(client.head_object(Bucket=bucket, Key=key)):
        raise ReleaseError(f"COS 上传后校验失败：{key}")
    print(f"  上传并校验：{key}")


def publish_release(client, bucket: str, manifest: dict, artifacts: tuple[Path, Path, Path]) -> None:
    """任一产物失败都不切换 latest；相同内容可重试，异内容禁止复用版本号。"""
    try:
        check_remote_version(remote_manifest(client, bucket), manifest)
        for key, path in zip(artifact_keys(manifest["version"]), artifacts):
            upload_immutable(client, bucket, key, path)
        payload = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        upload_immutable(client, bucket, f"releases/{manifest['version']}.json", payload)
        # 长时间上传期间可能已有另一个发版完成，切换前再检查一次。
        current = remote_manifest(client, bucket)
        check_remote_version(current, manifest)
        if current != manifest:
            client.put_object(Bucket=bucket, Key="update.json", Body=payload, EnableMD5=True,
                              ContentType="application/json; charset=utf-8",
                              CacheControl="no-cache, max-age=0")
        if remote_manifest(client, bucket) != manifest:
            raise ReleaseError("update.json 发布后的回读校验失败，请检查远端状态后续跑")
    except ReleaseError:
        raise
    except Exception:
        raise ReleaseError("COS 上传或校验失败（凭据和远端异常详情已隐藏），可使用 --resume 重试") from None


def main() -> int:
    configure_console()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--artifacts-dir")
    parser.add_argument("--installer", "--nsis-zip", dest="nsis_zip", help="Tauri v2 的 NSIS .exe 安装包")
    parser.add_argument("--sig-file")
    parser.add_argument("--portable-exe")
    parser.add_argument("--notes", default=None)
    parser.add_argument("--notes-file", type=Path, default=ROOT / "UPDATE.md")
    parser.add_argument("--dry-run", action="store_true", help="仅校验产物并生成本地清单，不读取 .env、不联网")
    args = parser.parse_args()
    try:
        args.version = args.version.removeprefix("v")
        artifacts = resolve_artifacts(args)
        config = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
        signature = verify_updater_signature(artifacts[0], artifacts[1], config["plugins"]["updater"]["pubkey"])
        notes = args.notes if args.notes is not None else args.notes_file.read_text(encoding="utf-8").strip()
        if not notes:
            raise ReleaseError("更新日志不能为空")
        env = {} if args.dry_run else load_release_env()
        base_url = public_base_url(env.get("COS_CDN_DOMAIN", "updater.anzu.link"))
        keys = artifact_keys(args.version)
        output = ROOT / ".release" / args.version / "update.json"
        previous = json.loads(output.read_text(encoding="utf-8")) if output.exists() else {}
        manifest = generate_update_json(args.version, signature, f"{base_url}/{keys[0]}",
                                        f"{base_url}/{keys[2]}", notes, previous.get("pub_date"))
        write_json(output, manifest)
        if args.dry_run:
            print(f"预演完成，清单：{output}")
            return 0
        publish_release(create_client(env), env["COS_BUCKET"], manifest, artifacts)
        print(f"发布完成：{base_url}/update.json")
        return 0
    except (ReleaseError, ValueError) as error:
        print(f"错误：{error}", file=sys.stderr)
        return 1
    except (OSError, KeyError):
        print("错误：无法读取发版文件或配置", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

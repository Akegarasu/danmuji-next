"""发版共用功能：运行时加载配置、隐藏凭据、校验产物。"""

from __future__ import annotations

import base64
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
ENV_NAMES = (
    "TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    "COS_SECRET_ID", "COS_SECRET_KEY", "COS_BUCKET", "COS_REGION", "COS_CDN_DOMAIN",
)
SECRET_NAMES = ("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
                "COS_SECRET_ID", "COS_SECRET_KEY")


class ReleaseError(RuntimeError):
    """可直接向用户展示、且不包含凭据的错误。"""


def configure_console() -> None:
    # Windows 管道默认可能使用 GBK；供 agent 读取的中文日志统一使用 UTF-8。
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")


def load_release_env(root: Path = ROOT) -> dict[str, str]:
    """仅在执行时读取 .env；已有进程环境变量优先，不做变量插值。"""
    env = os.environ.copy()
    path = root / ".env"
    if path.is_file():
        try:
            from dotenv.parser import parse_stream
        except ImportError:
            raise ReleaseError("请先运行 python -m pip install -r scripts/requirements-release.txt") from None
        with path.open(encoding="utf-8-sig") as stream:
            for binding in parse_stream(stream):
                if binding.error:
                    raise ReleaseError(f".env 第 {binding.original.line} 行格式错误（内容已隐藏）")
                if binding.key in ENV_NAMES and binding.value is not None:
                    env.setdefault(binding.key, binding.value)
    env.setdefault("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", "")
    env["PYTHONUTF8"] = "1"
    env["PYTHONIOENCODING"] = "utf-8"
    return env


def require_env(env: dict[str, str], names: tuple[str, ...]) -> None:
    missing = [name for name in names if not env.get(name)]
    if missing:
        raise ReleaseError("缺少环境变量：" + ", ".join(missing))


def redact(text: str, env: dict[str, str]) -> str:
    values = {env.get(name, "") for name in SECRET_NAMES}
    values |= {line for value in tuple(values) for line in value.splitlines() if line}
    for value in sorted(values - {""}, key=len, reverse=True):
        text = text.replace(value, "[REDACTED]")
    return text


def run_command(command: list[str], root: Path, env: dict[str, str]) -> None:
    """凭据只通过子进程环境传递；日志脱敏，不输出完整命令或异常对象。"""
    try:
        with subprocess.Popen(command, cwd=root, env=env, stdin=subprocess.DEVNULL,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, encoding="utf-8", errors="replace") as process:
            assert process.stdout is not None
            for line in process.stdout:
                print(redact(line, env), end="", flush=True)
            if process.wait() != 0:
                raise ReleaseError(f"构建命令失败，退出码 {process.returncode}")
    except OSError:
        raise ReleaseError("无法启动构建命令，请检查 pnpm、Node.js、Rust 和 MSVC 是否已安装") from None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def verify_updater_signature(installer: Path, signature_file: Path, public_key: str) -> str:
    """用配置中的公钥校验 Tauri 的 base64 Minisign 签名及 trusted comment。"""
    try:
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    except ImportError:
        raise ReleaseError("缺少签名校验依赖，请安装 scripts/requirements-release.txt") from None
    try:
        signature = signature_file.read_text(encoding="utf-8").strip()
        public_lines = base64.b64decode(public_key, validate=True).decode("utf-8").splitlines()
        signature_lines = base64.b64decode(signature, validate=True).decode("utf-8").splitlines()
        key = base64.b64decode(public_lines[1], validate=True)
        packet = base64.b64decode(signature_lines[1], validate=True)
        global_signature = base64.b64decode(signature_lines[3], validate=True)
        if (len(key) != 42 or len(packet) != 74 or len(global_signature) != 64
                or key[:2] not in (b"Ed", b"ED") or packet[:2] not in (b"Ed", b"ED")
                or key[2:10] != packet[2:10]
                or not signature_lines[2].startswith("trusted comment: ")):
            raise ValueError("invalid signature")
        verifier = Ed25519PublicKey.from_public_bytes(key[10:])
        if packet[:2] == b"ED":
            digest = hashlib.blake2b()
            with installer.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
            payload = digest.digest()
        else:
            payload = installer.read_bytes()
        verifier.verify(packet[10:], payload)
        comment = signature_lines[2][len("trusted comment: "):].encode("utf-8")
        verifier.verify(global_signature, packet[10:] + comment)
        return signature
    except Exception:
        raise ReleaseError("安装包签名校验失败：请检查产物、.sig 与 tauri.conf.json 中的公钥是否匹配") from None

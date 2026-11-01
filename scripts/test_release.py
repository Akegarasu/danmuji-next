"""验证发版副作用与失败边界；只使用临时仓库、测试密钥和内存 COS。"""

from __future__ import annotations

import base64
import hashlib
import io
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from contextlib import ExitStack, redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from qcloud_cos.cos_comm import mapped

import bump_version_and_build as versions
import cos_upload
import release
import release_support as support


class NotFound(Exception):
    def get_status_code(self):
        return 404


class FakeCos:
    def __init__(self):
        self.objects = {}
        self.metadata = {}
        self.writes = []
        self.fail_key = None

    def head_object(self, Bucket, Key):
        if Key not in self.objects:
            raise NotFound()
        return {"Content-Length": str(len(self.objects[Key])),
                "x-cos-meta-sha256": self.metadata.get(Key, {}).get("x-cos-meta-sha256", "")}

    def get_object(self, Bucket, Key):
        if Key not in self.objects:
            raise NotFound()
        return {"Body": SimpleNamespace(get_raw_stream=lambda: io.BytesIO(self.objects[Key]))}

    def put_object(self, Bucket, Key, Body, Metadata=None, **kwargs):
        if Key == self.fail_key:
            raise OSError("模拟网络中断")
        self.writes.append(Key)
        self.objects[Key] = Body
        # 使用 SDK 的真实 HTTP 头映射，避免模拟层接受线上不生效的自定义元数据。
        self.metadata[Key] = mapped({"Metadata": Metadata or {}})

    def upload_file(self, Bucket, Key, LocalFilePath, **kwargs):
        self.put_object(Bucket, Key, Path(LocalFilePath).read_bytes(), **kwargs)


def test_key():
    key = Ed25519PrivateKey.generate()
    raw = key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    public = base64.b64encode(b"untrusted comment: test\n" + base64.b64encode(b"Ed" + b"testkey1" + raw) + b"\n").decode()
    return key, public


def sign_fixture(path: Path, key, prehashed=True):
    data = hashlib.blake2b(path.read_bytes()).digest() if prehashed else path.read_bytes()
    signature = key.sign(data)
    comment = b"test release"
    packet = (b"ED" if prehashed else b"Ed") + b"testkey1" + signature
    text = b"untrusted comment: test\n" + base64.b64encode(packet) + b"\ntrusted comment: " + comment
    text += b"\n" + base64.b64encode(key.sign(signature + comment)) + b"\n"
    sig_path = path.with_name(path.name + ".sig")
    sig_path.write_text(base64.b64encode(text).decode(), encoding="utf-8")
    return sig_path


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        parent = (support.ROOT / ".release/tests").resolve()
        parent.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="release-", dir=parent)
        self.root = Path(self.temporary.name).resolve()
        self.root.relative_to(parent)
        self.addCleanup(self.temporary.cleanup)
        self.key, self.public_key = test_key()
        (self.root / "src-tauri").mkdir()
        (self.root / ".gitignore").write_text(".release/\n.env\n.vscode/\n", encoding="utf-8")
        (self.root / "package.json").write_text('{\n  "name": "danmuji-next",\n  "version": "2.3.0"\n}\n', encoding="utf-8")
        (self.root / "src-tauri/Cargo.toml").write_text('[package]\nname = "danmuji-next"\nversion = "2.3.0"\n\n[dependencies]\nother = "1.0"\n', encoding="utf-8")
        (self.root / "src-tauri/Cargo.lock").write_text('version = 4\n\n[[package]]\nname = "danmuji-next"\nversion = "2.3.0"\ndependencies = [\n "other",\n]\n\n[[package]]\nname = "other"\nversion = "2.3.0"\n', encoding="utf-8")
        config = {"version": "2.3.0", "bundle": {"createUpdaterArtifacts": True},
                  "plugins": {"updater": {"pubkey": self.public_key}}}
        (self.root / "src-tauri/tauri.conf.json").write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
        self.git("init", "-q")
        self.git("config", "user.email", "release-test@example.invalid")
        self.git("config", "user.name", "Release Test")
        self.git("config", "core.autocrlf", "false")
        self.commit("initial")
        self.git("tag", "v2.3.0")
        versions.update_versions("2.3.0", "2.7.3", self.root)
        self.base = self.commit("chore: release 2.7.3")
        (self.root / "feature.txt").write_text("first\n", encoding="utf-8")
        self.feature = self.commit("feat: mirror support\n\nHandle incoming mirror events.")
        (self.root / "feature.txt").write_text("second\n", encoding="utf-8")
        self.head = self.commit("fix: filter mirror messages")

    def git(self, *args):
        result = subprocess.run(["git", *args], cwd=self.root, capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(result.returncode, 0, "临时仓库 Git 操作失败")
        return result.stdout.strip()

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def prepare(self, part="patch"):
        plan = release.create_plan(self.root, part)
        support.write_json(self.root / ".release/plan.json", plan)
        notes = f"# {plan['version']} 更新\n\n- 支持显示跨房弹幕，可在设置中自由开关。"
        (self.root / "UPDATE.md").write_text(notes + "\n", encoding="utf-8")
        return plan

    def build(self, root, env):
        version = versions.get_current_version(root)
        directory = root / "src-tauri/target/release"
        (directory / "bundle/nsis").mkdir(parents=True, exist_ok=True)
        installer = directory / "bundle/nsis" / cos_upload.artifact_keys(version)[0]
        installer.write_bytes(b"test installer " + version.encode())
        sign_fixture(installer, self.key)
        (directory / "danmuji-next.exe").write_bytes(b"test portable " + version.encode())

    def pipeline_mocks(self, client, build=None):
        stack = ExitStack()
        stack.enter_context(patch.object(release, "preflight"))
        stack.enter_context(patch.object(release, "load_release_env", return_value={"COS_BUCKET": "test-bucket"}))
        stack.enter_context(patch.object(release, "create_client", return_value=client))
        builder = stack.enter_context(patch.object(release, "run_tauri_build", side_effect=build or self.build))
        # 构建目录是 Tauri 标准忽略路径。
        (self.root / "src-tauri/.gitignore").write_text("/target/\n", encoding="utf-8")
        self.git("add", "src-tauri/.gitignore")
        self.git("commit", "-qm", "chore: ignore build artifacts")
        self.prepare()
        return stack, builder

    def invoke(self, *args):
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return release.main(list(args), self.root)

    def test_semver_aliases_and_resets(self):
        for part, expected in (("patch", "2.7.4"), ("0.0.1", "2.7.4"), ("minor", "2.8.0"),
                               ("0.1", "2.8.0"), ("major", "3.0.0"), ("1", "3.0.0")):
            with self.subTest(part=part):
                self.assertEqual(versions.bump_version("2.7.3", part), expected)
        with self.assertRaises(ValueError):
            versions.ensure_semver("02.7.3")

    def test_bump_updates_all_manifests_and_only_own_lock_entry(self):
        versions.update_versions("2.7.3", "2.8.0", self.root)
        self.assertEqual(versions.get_current_version(self.root), "2.8.0")
        lock = (self.root / versions.LOCK_FILE).read_text(encoding="utf-8")
        self.assertEqual(versions.version_from_text(versions.LOCK_FILE, lock), "2.8.0")
        self.assertIn('name = "other"\nversion = "2.3.0"', lock)
        self.assertIn('version = "2.8.0"\ndependencies', lock)

    def test_bump_preserves_crlf(self):
        path = self.root / versions.LOCK_FILE
        path.write_bytes(path.read_bytes().replace(b"\n", b"\r\n"))
        before = path.read_bytes()
        versions.update_versions("2.7.3", "2.7.4", self.root)
        self.assertEqual(path.read_bytes(), before.replace(b'"2.7.3"', b'"2.7.4"', 1))

    def test_mismatch_does_not_partially_write(self):
        path = self.root / versions.LOCK_FILE
        path.write_bytes(path.read_bytes().replace(b'"2.7.3"', b'"1.0.0"'))
        before = {p: (self.root / p).read_bytes() for p in versions.VERSION_FILES}
        with self.assertRaises(ValueError):
            versions.update_versions("2.7.3", "2.7.4", self.root)
        self.assertEqual(before, {p: (self.root / p).read_bytes() for p in before})

    def test_stale_tag_uses_current_version_commit(self):
        plan = release.create_plan(self.root)
        self.assertEqual(plan["base"], self.base)
        self.assertEqual([c["sha"] for c in plan["commits"]], [self.feature, self.head])
        self.assertIn("Handle incoming", plan["commits"][0]["message"])

    def test_exact_tag_and_explicit_base(self):
        self.git("tag", "v2.7.3", self.feature)
        self.assertEqual(release.create_plan(self.root)["base"], self.feature)
        self.assertEqual(release.create_plan(self.root, since=self.base)["base"], self.base)

    def test_success_record_takes_priority(self):
        support.write_json(self.root / ".release/last-release.json", {"version": "2.7.3", "head": self.feature})
        self.assertEqual(release.create_plan(self.root)["base"], self.feature)

    def test_concise_user_notes_need_no_commit_metadata(self):
        plan = self.prepare()
        path = self.root / "UPDATE.md"
        notes = f"# {plan['version']} 更新\n\n- 修复闪退。"
        path.write_text(notes, encoding="utf-8")
        self.assertEqual(release.validate_notes(self.root, plan), notes)

    def test_notes_reject_wrong_version_or_unfinished_content(self):
        plan = self.prepare()
        path = self.root / "UPDATE.md"
        invalid = ("# 2.7.3 更新\n\n- 修复闪退。", "# 2.7.4 更新",
                   "# 2.7.4 更新\n\n## 更新内容\n<!-- 仅内部备注 -->",
                   "# 2.7.4 更新\n\n- 待补充")
        for notes in invalid:
            with self.subTest(notes=notes):
                path.write_text(notes, encoding="utf-8")
                with self.assertRaises(support.ReleaseError):
                    release.validate_notes(self.root, plan)

    def test_dry_run_never_loads_credentials_or_mutates_files(self):
        self.prepare()
        before = {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file() and ".git" not in p.parts}
        with patch.object(release, "load_release_env", side_effect=AssertionError("不应读取凭据")):
            self.assertEqual(self.invoke("run", "--dry-run"), 0)
        after = {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file() and ".git" not in p.parts}
        self.assertEqual(before, after)

    def test_plan_dry_run_writes_nothing(self):
        self.assertEqual(self.invoke("plan", "--dry-run"), 0)
        self.assertFalse((self.root / ".release").exists())

    def test_dirty_source_and_stale_head_block_publication(self):
        self.prepare()
        (self.root / "feature.txt").write_text("uncommitted", encoding="utf-8")
        self.assertEqual(self.invoke("run", "--dry-run"), 1)
        self.commit("new feature")
        self.assertEqual(self.invoke("run", "--dry-run"), 1)
        self.assertEqual(versions.get_current_version(self.root), "2.7.3")

    def test_env_precedence_multiline_and_no_interpolation(self):
        (self.root / ".env").write_text('export COS_SECRET_ID="file-id"\nCOS_SECRET_KEY="${UNEXPANDED}#literal"\nTAURI_SIGNING_PRIVATE_KEY="line1\nline2"\nTAURI_SIGNING_PRIVATE_KEY_PASSWORD=""\n', encoding="utf-8")
        with patch.dict(os.environ, {"COS_SECRET_ID": "process-id"}, clear=True):
            env = support.load_release_env(self.root)
        self.assertEqual(env["COS_SECRET_ID"], "process-id")
        self.assertEqual(env["COS_SECRET_KEY"], "${UNEXPANDED}#literal")
        self.assertEqual(env["TAURI_SIGNING_PRIVATE_KEY"], "line1\nline2")
        self.assertEqual(env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"], "")
        self.assertNotIn("process-id", support.redact("error process-id line2", env))
        self.assertNotIn("line2", support.redact("error process-id line2", env))

    def test_malformed_env_does_not_echo_content(self):
        (self.root / ".env").write_text('COS_SECRET_KEY="private-test-value\n', encoding="utf-8")
        with self.assertRaises(support.ReleaseError) as caught:
            support.load_release_env(self.root)
        self.assertNotIn("private-test-value", str(caught.exception))

    def test_signature_verifies_content_and_trusted_comment(self):
        artifact = self.root / "installer.exe"
        artifact.write_bytes(b"signed test content")
        for prehashed in (True, False):
            signature = sign_fixture(artifact, self.key, prehashed)
            support.verify_updater_signature(artifact, signature, self.public_key)
        content = base64.b64decode(signature.read_text()).replace(b"test release", b"evil release")
        signature.write_text(base64.b64encode(content).decode())
        with self.assertRaises(support.ReleaseError):
            support.verify_updater_signature(artifact, signature, self.public_key)
        sign_fixture(artifact, self.key)
        artifact.write_bytes(b"tampered")
        with self.assertRaises(support.ReleaseError):
            support.verify_updater_signature(artifact, signature, self.public_key)

    def test_installed_tauri_signer_interoperability(self):
        cli = support.ROOT / "node_modules/@tauri-apps/cli/tauri.js"
        if not cli.is_file() or not shutil.which("node"):
            self.skipTest("未安装 Tauri CLI")
        key_path = self.root / "ephemeral.key"
        # 不加载项目 .env，也不把签名工具输出（可能含测试密钥）打印出来。
        env = {k: v for k, v in os.environ.items() if not k.startswith("TAURI_SIGNING_")}
        env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = ""
        generated = subprocess.run(["node", str(cli), "signer", "generate", "--ci", "-w", str(key_path)],
                                   capture_output=True, env=env, stdin=subprocess.DEVNULL)
        self.assertEqual(generated.returncode, 0)
        artifact = self.root / "actual-tauri-signed.exe"
        artifact.write_bytes(b"local interoperability fixture")
        signed = subprocess.run(["node", str(cli), "signer", "sign", "-f", str(key_path), str(artifact)],
                                capture_output=True, env=env, stdin=subprocess.DEVNULL)
        self.assertEqual(signed.returncode, 0)
        public = key_path.with_name(key_path.name + ".pub").read_text(encoding="utf-8").strip()
        support.verify_updater_signature(artifact, artifact.with_name(artifact.name + ".sig"), public)

    def test_resolver_rejects_old_version_or_ambiguous_artifacts(self):
        self.build(self.root, {})
        directory = self.root / "src-tauri/target/release"
        args = SimpleNamespace(version="2.7.4", artifacts_dir=directory, nsis_zip=None, sig_file=None, portable_exe=None)
        with self.assertRaises(support.ReleaseError):
            cos_upload.resolve_artifacts(args)
        args.version = "2.7.3"
        files = cos_upload.resolve_artifacts(args)
        shutil.copy2(files[0], directory / files[0].name)
        with self.assertRaises(support.ReleaseError):
            cos_upload.resolve_artifacts(args)

    def test_cdn_supports_hostname_or_https_and_rejects_credentials(self):
        self.assertEqual(cos_upload.public_base_url("cdn.example.com"), "https://cdn.example.com")
        self.assertEqual(cos_upload.public_base_url("https://cdn.example.com/"), "https://cdn.example.com")
        for value in ("http://cdn.example.com", "https://secret@cdn.example.com", "https://cdn.example.com/path"):
            with self.assertRaises(support.ReleaseError):
                cos_upload.public_base_url(value)

    def test_build_failure_resumes_without_bumping_again(self):
        client = FakeCos()
        def build_then_succeed(root, env):
            if builder.call_count == 1:
                raise support.ReleaseError("模拟构建失败")
            self.build(root, env)
        stack, builder = self.pipeline_mocks(client, build_then_succeed)
        with stack:
            self.assertEqual(self.invoke("run"), 1)
            self.assertEqual(versions.get_current_version(self.root), "2.7.4")
            self.assertFalse(client.writes)
            self.assertEqual(self.invoke("run", "--resume"), 0)
            self.assertEqual(builder.call_count, 2)
        self.assertEqual(versions.get_current_version(self.root), "2.7.4")
        self.assertEqual(client.writes[-1], "update.json")

    def test_upload_failure_keeps_latest_and_resumes_without_rebuild(self):
        client = FakeCos()
        old = {"version": "2.7.3"}
        client.objects["update.json"] = json.dumps(old).encode()
        client.fail_key = cos_upload.artifact_keys("2.7.4")[2]
        stack, builder = self.pipeline_mocks(client)
        with stack:
            self.assertEqual(self.invoke("run"), 1)
            self.assertEqual(json.loads(client.objects["update.json"]), old)
            first_writes = client.writes[:]
            client.fail_key = None
            self.assertEqual(self.invoke("run", "--resume"), 0)
            self.assertEqual(builder.call_count, 1)
            self.assertEqual(self.invoke("run", "--resume"), 0)
        self.assertEqual(client.writes.count(first_writes[0]), 1)
        self.assertEqual(client.writes[-1], "update.json")
        self.assertEqual(json.loads(client.objects["update.json"])["version"], "2.7.4")
        self.assertEqual(json.loads(client.objects["update.json"])["notes"],
                         (self.root / "UPDATE.md").read_text(encoding="utf-8").strip())
        record = json.loads((self.root / ".release/last-release.json").read_text())
        self.assertEqual(record["version"], "2.7.4")

    def test_remote_newer_release_stops_before_bump(self):
        client = FakeCos()
        client.objects["update.json"] = b'{"version":"3.0.0"}'
        stack, builder = self.pipeline_mocks(client)
        with stack:
            self.assertEqual(self.invoke("run"), 1)
            builder.assert_not_called()
        self.assertEqual(versions.get_current_version(self.root), "2.7.3")

    def test_tampered_staged_artifacts_are_not_uploaded(self):
        client = FakeCos()
        stack, builder = self.pipeline_mocks(client)
        with stack:
            self.assertEqual(self.invoke("run", "--no-upload"), 0)
            self.assertFalse(client.writes)
            (self.root / ".release/2.7.4" / cos_upload.artifact_keys("2.7.4")[0]).write_bytes(b"tampered")
            self.assertEqual(self.invoke("run", "--resume"), 1)
            self.assertFalse(client.writes)
            self.assertEqual(builder.call_count, 1)

    def test_reset_before_upload_restores_versions_and_preserves_notes(self):
        client = FakeCos()
        stack, builder = self.pipeline_mocks(client)
        with stack:
            self.assertEqual(self.invoke("run", "--no-upload"), 0)
            notes = (self.root / "UPDATE.md").read_bytes()
            self.assertEqual(self.invoke("reset"), 0)
        self.assertEqual(versions.get_current_version(self.root), "2.7.3")
        self.assertEqual((self.root / "UPDATE.md").read_bytes(), notes)
        self.assertFalse((self.root / ".release/state.json").exists())
        self.assertEqual(len(list((self.root / ".release/abandoned").glob("*.json"))), 1)

    def test_reset_after_upload_started_is_rejected(self):
        client = FakeCos()
        client.fail_key = cos_upload.artifact_keys("2.7.4")[2]
        stack, builder = self.pipeline_mocks(client)
        with stack:
            self.assertEqual(self.invoke("run"), 1)
            self.assertEqual(self.invoke("reset"), 1)
        self.assertEqual(versions.get_current_version(self.root), "2.7.4")
        self.assertTrue((self.root / ".release/state.json").exists())

    def test_conflicting_immutable_object_is_not_overwritten(self):
        client = FakeCos()
        client.objects["artifact.exe"] = b"already published"
        with self.assertRaises(support.ReleaseError):
            cos_upload.upload_immutable(client, "test", "artifact.exe", b"different")
        self.assertEqual(client.objects["artifact.exe"], b"already published")
        self.assertFalse(client.writes)

    def test_publication_recovers_after_latest_write_lost_response(self):
        client = FakeCos()
        stack, builder = self.pipeline_mocks(client)
        with stack:
            real_publish = release.publish_release
            def publish_then_fail(*args):
                real_publish(*args)
                raise support.ReleaseError("模拟写入成功后的连接中断")
            with patch.object(release, "publish_release", side_effect=publish_then_fail):
                self.assertEqual(self.invoke("run"), 1)
            self.assertIn("update.json", client.objects)
            count = len(client.writes)
            self.assertEqual(self.invoke("run", "--resume"), 0)
            self.assertEqual(len(client.writes), count)
            self.assertEqual(builder.call_count, 1)


if __name__ == "__main__":
    unittest.main()

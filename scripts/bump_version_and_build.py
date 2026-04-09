#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
PACKAGE_NAME = "danmuji-next"
VERSION_PATTERN = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")


def read_lines(path: Path) -> list[str]:
    return path.read_text(encoding="utf-8").splitlines(keepends=True)


def write_lines(path: Path, lines: list[str]) -> None:
    path.write_text("".join(lines), encoding="utf-8")


def extract_json_version(path: Path) -> str:
    for line in read_lines(path):
        match = re.match(r'^(\s*"version":\s*")([^"]+)(".*)$', line)
        if match:
            return match.group(2)
    raise ValueError(f"Could not find JSON version in {path}")


def replace_json_version(path: Path, old_version: str, new_version: str) -> None:
    lines = read_lines(path)
    for index, line in enumerate(lines):
        match = re.match(r'^(\s*"version":\s*")([^"]+)(".*)$', line)
        if match:
            if match.group(2) != old_version:
                raise ValueError(
                    f"Version mismatch in {path}: expected {old_version}, found {match.group(2)}"
                )
            lines[index] = f"{match.group(1)}{new_version}{match.group(3)}\n"
            write_lines(path, lines)
            return
    raise ValueError(f"Could not update JSON version in {path}")


def extract_cargo_toml_version(path: Path) -> str:
    in_package = False
    for line in read_lines(path):
        stripped = line.strip()
        if stripped.startswith("["):
            in_package = stripped == "[package]"
            continue
        if in_package:
            match = re.match(r'^version\s*=\s*"([^"]+)"$', stripped)
            if match:
                return match.group(1)
    raise ValueError(f"Could not find package version in {path}")


def replace_cargo_toml_version(path: Path, old_version: str, new_version: str) -> None:
    lines = read_lines(path)
    in_package = False
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("["):
            in_package = stripped == "[package]"
            continue
        if in_package:
            match = re.match(r'^(\s*version\s*=\s*")([^"]+)(".*)$', line)
            if match:
                if match.group(2) != old_version:
                    raise ValueError(
                        f"Version mismatch in {path}: expected {old_version}, found {match.group(2)}"
                    )
                lines[index] = f"{match.group(1)}{new_version}{match.group(3)}\n"
                write_lines(path, lines)
                return
    raise ValueError(f"Could not update package version in {path}")


def extract_cargo_lock_version(path: Path) -> str:
    current_name = None
    for line in read_lines(path):
        stripped = line.strip()
        if stripped == "[[package]]":
            current_name = None
            continue
        name_match = re.match(r'^name\s*=\s*"([^"]+)"$', stripped)
        if name_match:
            current_name = name_match.group(1)
            continue
        version_match = re.match(r'^version\s*=\s*"([^"]+)"$', stripped)
        if version_match and current_name == PACKAGE_NAME:
            return version_match.group(1)
    raise ValueError(f"Could not find {PACKAGE_NAME} version in {path}")


def replace_cargo_lock_version(path: Path, old_version: str, new_version: str) -> None:
    lines = read_lines(path)
    current_name = None
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped == "[[package]]":
            current_name = None
            continue
        name_match = re.match(r'^name\s*=\s*"([^"]+)"$', stripped)
        if name_match:
            current_name = name_match.group(1)
            continue
        version_match = re.match(r'^(\s*version\s*=\s*")([^"]+)(".*)$', line)
        if version_match and current_name == PACKAGE_NAME:
            if version_match.group(2) != old_version:
                raise ValueError(
                    f"Version mismatch in {path}: expected {old_version}, found {version_match.group(2)}"
                )
            lines[index] = f"{version_match.group(1)}{new_version}{version_match.group(3)}"
            write_lines(path, lines)
            return
    raise ValueError(f"Could not update {PACKAGE_NAME} version in {path}")


def ensure_semver(version: str) -> tuple[int, int, int]:
    match = VERSION_PATTERN.match(version)
    if not match:
        raise ValueError(f"Unsupported version format: {version}")
    return tuple(int(group) for group in match.groups())


def bump_version(version: str, part: str) -> str:
    major, minor, patch = ensure_semver(version)
    if part == "major":
        return f"{major + 1}.0.0"
    if part == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def get_current_version() -> str:
    versions = {
        "package.json": extract_json_version(ROOT / "package.json"),
        "src-tauri/Cargo.toml": extract_cargo_toml_version(ROOT / "src-tauri" / "Cargo.toml"),
        "src-tauri/tauri.conf.json": extract_json_version(ROOT / "src-tauri" / "tauri.conf.json"),
    }
    unique_versions = set(versions.values())
    if len(unique_versions) != 1:
        details = ", ".join(f"{path}={version}" for path, version in versions.items())
        raise ValueError(f"Version mismatch across files: {details}")
    return unique_versions.pop()


def update_versions(old_version: str, new_version: str) -> None:
    replace_json_version(ROOT / "package.json", old_version, new_version)
    replace_cargo_toml_version(ROOT / "src-tauri" / "Cargo.toml", old_version, new_version)
    replace_json_version(ROOT / "src-tauri" / "tauri.conf.json", old_version, new_version)
    replace_cargo_lock_version(ROOT / "src-tauri" / "Cargo.lock", old_version, new_version)


def run_tauri_build() -> None:
    subprocess.run(["pnpm", "exec", "tauri", "build"], cwd=ROOT, check=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Bump the app version and optionally run Tauri build."
    )
    parser.add_argument(
        "part",
        nargs="?",
        choices=("major", "minor", "patch"),
        default="patch",
        help="Version part to increment. Defaults to patch.",
    )
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="Only bump the version without running Tauri build.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the next version without changing files or running build.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    try:
        current_version = get_current_version()
        next_version = bump_version(current_version, args.part)
    except ValueError as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1

    print(f"Current version: {current_version}")
    print(f"Next version: {next_version}")

    if args.dry_run:
        return 0

    try:
        update_versions(current_version, next_version)
    except ValueError as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1

    print("Version files updated.")

    if args.no_build:
        return 0

    print("Running Tauri build...")
    try:
        run_tauri_build()
    except subprocess.CalledProcessError as error:
        print(f"Tauri build failed with exit code {error.returncode}.", file=sys.stderr)
        return error.returncode

    print("Tauri build completed successfully.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

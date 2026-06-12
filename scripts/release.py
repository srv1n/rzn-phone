#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from release_common import (
    ROOT,
    assert_version_sync,
    current_version,
    normalize_version,
    release_tag,
    set_version,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Cut a tagged GitHub release by bumping versions, committing, and pushing."
    )
    parser.add_argument("--version", required=True, help="Release version (for example 0.2.0)")
    parser.add_argument(
        "--branch",
        default="main",
        help="Branch required for release creation (default: main).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate release inputs and print planned actions without mutating git state.",
    )
    return parser.parse_args()


def run(cmd: list[str], *, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        capture_output=capture,
        check=check,
    )


def output(cmd: list[str]) -> str:
    return run(cmd, capture=True).stdout.strip()


def ensure_clean_tree() -> None:
    dirty = output(["git", "status", "--porcelain"])
    if dirty:
        raise RuntimeError("release requires a clean git tree")


def ensure_branch(branch: str) -> None:
    current = output(["git", "branch", "--show-current"])
    if current != branch:
        raise RuntimeError(
            f"release must run from '{branch}', but current branch is '{current}'"
        )


def ensure_tag_absent(tag: str) -> None:
    local = run(
        ["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}"],
        capture=True,
        check=False,
    )
    if local.returncode == 0:
        raise RuntimeError(f"tag {tag} already exists locally")

    remote = output(["git", "ls-remote", "--tags", "origin", tag])
    if remote:
        raise RuntimeError(f"tag {tag} already exists on origin")


def print_plan(version: str, tag: str, branch: str) -> None:
    print(f"current version: {current_version()}")
    print(f"next version:    {version}")
    print(f"tag:             {tag}")
    print(f"branch:          {branch}")
    print("planned actions:")
    print("  1. git pull --rebase origin <branch>")
    print("  2. update bundle + Cargo versions")
    print("  3. cargo test -p rzn_phone_worker")
    print("  4. git commit -m 'Release <tag>'")
    print("  5. git tag -a <tag> -m 'Release <tag>'")
    print("  6. git push origin HEAD")
    print("  7. git push origin <tag>")


def main() -> int:
    args = parse_args()
    target_version = normalize_version(args.version)
    target_tag = release_tag(target_version)

    assert_version_sync()
    ensure_clean_tree()
    ensure_branch(args.branch)
    ensure_tag_absent(target_tag)

    if current_version() == target_version:
        raise RuntimeError(f"version is already {target_version}")

    if args.dry_run:
        print_plan(target_version, target_tag, args.branch)
        return 0

    original_version = current_version()
    cargo_lock_path = ROOT / "Cargo.lock"
    original_cargo_lock = cargo_lock_path.read_text(encoding="utf-8")
    version_files_written = False

    try:
        run(["git", "pull", "--rebase", "origin", args.branch])
        set_version(target_version)
        version_files_written = True
        run(["cargo", "test", "-p", "rzn_phone_worker"])
        run(
            [
                "git",
                "add",
                "plugin_bundle/rzn-phone.bundle.json",
                "crates/rzn_phone_worker/Cargo.toml",
                "Cargo.lock",
            ]
        )
        run(["git", "commit", "-m", f"Release {target_tag}"])
        version_files_written = False
        run(["git", "tag", "-a", target_tag, "-m", f"Release {target_tag}"])
        run(["git", "push", "origin", "HEAD"])
        run(["git", "push", "origin", target_tag])
    except Exception:
        if version_files_written:
            set_version(original_version)
            cargo_lock_path.write_text(original_cargo_lock, encoding="utf-8")
        raise

    print(f"released {target_tag}")
    print("GitHub Actions will build artifacts, generate release notes, and publish the GitHub release.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"release failed: {exc}", file=sys.stderr)
        raise SystemExit(1)

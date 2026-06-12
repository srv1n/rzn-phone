#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import posixpath
import re
import sys
import tarfile
import warnings
from pathlib import Path


SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")
SAFE_MODE_MASK = 0o7777
SPECIAL_MODE_BITS = 0o7000


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_sha256(sum_path: Path, archive_name: str) -> str:
    fallback: str | None = None
    for line in sum_path.read_text(encoding="utf-8").splitlines():
        fields = line.strip().split()
        if not fields:
            continue
        digest = fields[0]
        if not SHA256_RE.fullmatch(digest):
            continue
        if len(fields) == 1:
            fallback = digest.lower()
            continue
        names = [field.lstrip("*") for field in fields[1:]]
        if archive_name in names:
            return digest.lower()
    if fallback is not None:
        return fallback
    raise SystemExit(f"no sha256 entry found for {archive_name} in {sum_path}")


def verify_sha256(archive_path: Path, sum_path: Path) -> None:
    expected = expected_sha256(sum_path, archive_path.name)
    actual = sha256_file(archive_path)
    if actual.lower() != expected:
        raise SystemExit(
            f"sha256 mismatch for {archive_path.name}: expected {expected}, got {actual}"
        )


def validate_member(info: tarfile.TarInfo, root_name: str) -> None:
    name = info.name
    normalized = posixpath.normpath(name)
    if (
        not name
        or name.startswith("/")
        or normalized == "."
        or normalized.startswith("../")
        or "/../" in normalized
        or normalized != name.rstrip("/")
    ):
        raise SystemExit(f"unsafe archive path: {name!r}")

    parts = normalized.split("/")
    if parts[0] != root_name:
        raise SystemExit(f"archive member is outside {root_name}: {name!r}")

    if info.issym() or info.islnk():
        raise SystemExit(f"archive links are not allowed: {name!r}")
    if not (info.isdir() or info.isfile()):
        raise SystemExit(f"archive member type is not allowed: {name!r}")

    mode = info.mode & SAFE_MODE_MASK
    if mode & SPECIAL_MODE_BITS:
        raise SystemExit(f"archive member has special mode bits: {name!r}")
    if mode & 0o002:
        raise SystemExit(f"archive member is world-writable: {name!r}")


def safe_extract(archive_path: Path, dest_dir: Path, root_name: str) -> None:
    with tarfile.open(archive_path, "r:gz") as archive:
        members = archive.getmembers()
        if not members:
            raise SystemExit("archive is empty")
        for member in members:
            validate_member(member, root_name)
        with warnings.catch_warnings():
            warnings.filterwarnings("ignore", category=DeprecationWarning)
            archive.extractall(dest_dir, members=members)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify release archive checksums and safely extract tarballs."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    verify_parser = subparsers.add_parser("verify-sha256")
    verify_parser.add_argument("--archive", required=True, type=Path)
    verify_parser.add_argument("--sha256", required=True, type=Path)

    extract_parser = subparsers.add_parser("safe-extract")
    extract_parser.add_argument("--archive", required=True, type=Path)
    extract_parser.add_argument("--dest", required=True, type=Path)
    extract_parser.add_argument("--root-name", required=True)

    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "verify-sha256":
        verify_sha256(args.archive, args.sha256)
        return 0
    if args.command == "safe-extract":
        safe_extract(args.archive, args.dest, args.root_name)
        return 0
    raise AssertionError(args.command)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except tarfile.TarError as exc:
        print(f"invalid tar archive: {exc}", file=sys.stderr)
        raise SystemExit(1)

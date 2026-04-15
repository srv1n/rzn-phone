#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUNDLE_CONFIG = ROOT / "plugin_bundle" / "rzn-phone.bundle.json"
CARGO_MANIFEST = ROOT / "crates" / "rzn_phone_worker" / "Cargo.toml"

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
CARGO_VERSION_RE = re.compile(r'(?m)^version\s*=\s*"([^"]+)"\s*$')


def load_bundle_config() -> dict:
    return json.loads(BUNDLE_CONFIG.read_text(encoding="utf-8"))


def current_version() -> str:
    return str(load_bundle_config()["version"]).strip()


def cargo_version() -> str:
    match = CARGO_VERSION_RE.search(CARGO_MANIFEST.read_text(encoding="utf-8"))
    if not match:
        raise RuntimeError(f"could not find version in {CARGO_MANIFEST}")
    return match.group(1).strip()


def normalize_version(raw: str) -> str:
    version = raw.strip()
    if version.startswith("v"):
        version = version[1:]
    if not SEMVER_RE.fullmatch(version):
        raise RuntimeError(
            f"invalid version '{raw}'; expected semver like 0.2.0 or 0.2.0-rc.1"
        )
    return version


def release_tag(version: str) -> str:
    return f"v{normalize_version(version)}"


def assert_version_sync() -> None:
    bundle = current_version()
    cargo = cargo_version()
    if bundle != cargo:
        raise RuntimeError(
            f"version mismatch: bundle={bundle} cargo={cargo}. Fix version drift first."
        )


def set_version(raw: str) -> str:
    version = normalize_version(raw)

    config = load_bundle_config()
    config["version"] = version
    BUNDLE_CONFIG.write_text(
        json.dumps(config, indent=2, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )

    manifest = CARGO_MANIFEST.read_text(encoding="utf-8")
    updated_manifest, count = CARGO_VERSION_RE.subn(
        f'version = "{version}"',
        manifest,
        count=1,
    )
    if count != 1:
        raise RuntimeError(f"failed to update version in {CARGO_MANIFEST}")
    CARGO_MANIFEST.write_text(updated_manifest, encoding="utf-8")
    return version


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[1] not in {"current-version", "assert-sync"}:
        print("usage: release_common.py [current-version|assert-sync]", file=sys.stderr)
        return 1

    if argv[1] == "current-version":
        print(current_version())
        return 0

    assert_version_sync()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

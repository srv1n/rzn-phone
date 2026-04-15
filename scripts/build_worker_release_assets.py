#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import shutil
import tarfile
import zipfile
from hashlib import sha256
from pathlib import Path


FIXED_ZIP_DT = (1980, 1, 1, 0, 0, 0)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Package standalone rzn-phone worker release assets for GitHub releases."
    )
    parser.add_argument(
        "--config",
        default="plugin_bundle/rzn-phone.bundle.json",
        help="Bundle config path used as the metadata source.",
    )
    parser.add_argument("--platform", required=True, help="Release platform key.")
    parser.add_argument(
        "--binary",
        required=True,
        help="Path to the compiled worker binary for the selected platform.",
    )
    parser.add_argument(
        "--out",
        default="dist/worker-releases",
        help="Output root for standalone worker release assets.",
    )
    return parser.parse_args()


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def reset_dir(path: Path) -> None:
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True, exist_ok=True)


def copy_tree(src: Path, dest: Path) -> None:
    shutil.copytree(src, dest, dirs_exist_ok=True)


def copy_file(src: Path, dest: Path, mode: int | None = None) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dest)
    if mode is not None:
        dest.chmod(mode)


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def sha256_file(path: Path) -> str:
    digest = sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalized_tarinfo(info: tarfile.TarInfo) -> tarfile.TarInfo:
    info.uid = 0
    info.gid = 0
    info.uname = "root"
    info.gname = "root"
    info.mtime = 0
    return info


def build_tar_gz(source_dir: Path, archive_path: Path, root_name: str) -> None:
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive_path, "w:gz", format=tarfile.PAX_FORMAT) as archive:
        archive.add(source_dir, arcname=root_name, filter=normalized_tarinfo)


def build_zip(source_dir: Path, archive_path: Path, root_name: str) -> None:
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for path in sorted(source_dir.rglob("*")):
            if not path.is_file():
                continue
            rel = Path(root_name) / path.relative_to(source_dir)
            info = zipfile.ZipInfo(rel.as_posix(), FIXED_ZIP_DT)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (path.stat().st_mode & 0o777) << 16
            archive.writestr(info, path.read_bytes())


def unix_launcher() -> str:
    return """#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export RZN_PLUGIN_DIR="$ROOT"
export CLAUDE_PLUGIN_ROOT="$ROOT"
exec "$ROOT/libexec/rzn-phone-worker" "$@"
"""


def windows_cmd_launcher() -> str:
    return """@echo off
setlocal
set "ROOT=%~dp0.."
set "RZN_PLUGIN_DIR=%ROOT%"
set "CLAUDE_PLUGIN_ROOT=%ROOT%"
"%ROOT%\\libexec\\rzn-phone-worker.exe" %*
"""


def windows_ps1_launcher() -> str:
    return """$Root = Split-Path -Parent $PSScriptRoot
$env:RZN_PLUGIN_DIR = $Root
$env:CLAUDE_PLUGIN_ROOT = $Root
& (Join-Path $Root "libexec\\rzn-phone-worker.exe") @args
exit $LASTEXITCODE
"""


def package_readme(version: str, platform: str, binary_entrypoint: str) -> str:
    return f"""# rzn-phone worker bundle

Version: `{version}`
Platform: `{platform}`
Entrypoint: `{binary_entrypoint}`

This archive ships the standalone `rzn-phone-worker` plus packaged workflows, system metadata,
and examples so the worker can run outside the repo.

Reality check:
- Full local iOS automation still requires macOS, Xcode, Appium, and the XCUITest driver.
- Linux and Windows bundles are for controller, integration, and remote-host scenarios. They do not
  magically make Xcode exist on non-macOS machines.
"""


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parent.parent
    config = load_json((root / args.config).resolve())
    binary = Path(args.binary)
    if not binary.is_absolute():
        binary = (root / binary).resolve()
    if not binary.exists():
        raise SystemExit(f"worker binary not found: {binary}")

    plugin_id = str(config["id"]).strip()
    version = str(config["version"]).strip()
    platform = str(args.platform).strip()
    out_dir = (root / args.out / plugin_id / version / platform).resolve()
    package_dir = out_dir / "package"
    reset_dir(package_dir)

    workflow_dir = root / "crates" / "rzn_phone_worker" / "resources" / "workflows"
    systems_dir = root / "crates" / "rzn_phone_worker" / "resources" / "systems"
    examples_dir = root / "examples"

    is_windows = platform.startswith("windows_") or binary.suffix.lower() == ".exe"
    binary_name = "rzn-phone-worker.exe" if is_windows else "rzn-phone-worker"
    launcher_entry = (
        "bin\\rzn-phone-worker.cmd" if is_windows else "bin/rzn-phone-worker"
    )

    copy_file(binary, package_dir / "libexec" / binary_name, 0o755)
    copy_tree(workflow_dir, package_dir / "resources" / "workflows")
    copy_tree(systems_dir, package_dir / "resources" / "systems")
    copy_tree(examples_dir, package_dir / "examples")
    write_text(package_dir / "VERSION", version + "\n")
    write_text(package_dir / "README.md", package_readme(version, platform, launcher_entry))

    if is_windows:
        write_text(package_dir / "bin" / "rzn-phone-worker.cmd", windows_cmd_launcher())
        write_text(package_dir / "bin" / "rzn-phone-worker.ps1", windows_ps1_launcher())
        archive_name = f"{plugin_id}-worker-{version}-{platform}.zip"
        archive_path = out_dir / archive_name
        build_zip(package_dir, archive_path, f"{plugin_id}-worker")
    else:
        write_text(package_dir / "bin" / "rzn-phone-worker", unix_launcher())
        (package_dir / "bin" / "rzn-phone-worker").chmod(0o755)
        archive_name = f"{plugin_id}-worker-{version}-{platform}.tar.gz"
        archive_path = out_dir / archive_name
        build_tar_gz(package_dir, archive_path, f"{plugin_id}-worker")

    archive_sha = sha256_file(archive_path)
    write_text(out_dir / f"{archive_name}.sha256", f"{archive_sha}  {archive_name}\n")
    write_text(out_dir / "SHA256SUMS", f"{archive_sha}  {archive_name}\n")
    print(str(out_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

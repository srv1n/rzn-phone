#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import subprocess
import tarfile
from hashlib import sha256
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build installable rzn-phone release artifacts."
    )
    parser.add_argument(
        "--config",
        default="plugin_bundle/rzn-phone.bundle.json",
        help="Path to the bundle config used as the release metadata source.",
    )
    parser.add_argument(
        "--platform",
        default="macos_universal",
        help="Release platform key.",
    )
    parser.add_argument(
        "--out",
        default="dist/releases",
        help="Output root for installable release artifacts.",
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


def build_archive(source_dir: Path, archive_path: Path, root_name: str) -> None:
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive_path, "w:gz", format=tarfile.PAX_FORMAT) as archive:
        archive.add(source_dir, arcname=root_name, filter=normalized_tarinfo)


def resolve_workflow_metadata(workflow_dir: Path) -> list[dict]:
    workflows = []
    for workflow_path in sorted(workflow_dir.glob("*.json")):
        raw = load_json(workflow_path)
        workflows.append(
            {
                "name": raw.get("name", workflow_path.stem),
                "version": raw.get("version", ""),
                "path": f"resources/workflows/{workflow_path.name}",
            }
        )
    return workflows


def ensure_universal_binary(root: Path) -> Path:
    binary = root / "dist" / "bin" / "macos" / "universal" / "rzn-phone-worker"
    if binary.exists():
        return binary
    subprocess.run([str(root / "scripts" / "build_universal.sh")], check=True)
    if not binary.exists():
        raise SystemExit(f"expected universal binary at {binary}")
    return binary


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parent.parent
    config_path = (root / args.config).resolve()
    config = load_json(config_path)
    plugin_id = str(config["id"]).strip()
    version = str(config["version"]).strip()
    platform = str(args.platform).strip()
    out_dir = (root / args.out / plugin_id / version / platform).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    package_dir = out_dir / "package"
    workflow_pack_dir = out_dir / "workflow-pack"
    reset_dir(package_dir)
    reset_dir(workflow_pack_dir)

    binary = ensure_universal_binary(root)
    workflow_dir = root / "crates" / "rzn_phone_worker" / "resources" / "workflows"
    systems_dir = root / "crates" / "rzn_phone_worker" / "resources" / "systems"
    examples_dir = root / "examples"
    runtime_launcher = root / "scripts" / "rzn_phone_runtime.sh"
    installer = root / "scripts" / "install_rzn_phone.sh"

    copy_file(runtime_launcher, package_dir / "bin" / "rzn-phone", 0o755)
    copy_file(binary, package_dir / "libexec" / "rzn-phone-worker", 0o755)
    copy_tree(workflow_dir, package_dir / "resources" / "workflows")
    copy_tree(systems_dir, package_dir / "resources" / "systems")
    copy_tree(examples_dir, package_dir / "examples")
    write_text(package_dir / "VERSION", version + "\n")
    write_text(package_dir / "WORKFLOW_PACK_VERSION", version + "\n")

    workflows = resolve_workflow_metadata(workflow_dir)
    examples = [
        str(path.relative_to(examples_dir).as_posix())
        for path in sorted(examples_dir.rglob("*"))
        if path.is_file()
    ]
    systems = [
        f"resources/systems/{path.relative_to(systems_dir).as_posix()}"
        for path in sorted(systems_dir.rglob("*"))
        if path.is_file()
    ]

    copy_tree(workflow_dir, workflow_pack_dir / "resources" / "workflows")
    copy_tree(systems_dir, workflow_pack_dir / "resources" / "systems")
    copy_tree(examples_dir, workflow_pack_dir / "examples")
    write_text(workflow_pack_dir / "VERSION", version + "\n")
    write_text(
        workflow_pack_dir / "pack.json",
        json.dumps(
            {
                "pack_id": f"{plugin_id}-workflows",
                "version": version,
                "min_worker_version": version,
                "workflows": workflows,
                "examples": examples,
                "systems": systems,
            },
            indent=2,
            ensure_ascii=True,
        )
        + "\n",
    )

    archive_name = f"{plugin_id}-{version}-{platform}.tar.gz"
    workflow_archive_name = f"{plugin_id}-workflows-{version}.tar.gz"
    archive_path = out_dir / archive_name
    workflow_archive_path = out_dir / workflow_archive_name

    build_archive(package_dir, archive_path, plugin_id)
    build_archive(workflow_pack_dir, workflow_archive_path, f"{plugin_id}-workflows")

    archive_sha = sha256_file(archive_path)
    workflow_archive_sha = sha256_file(workflow_archive_path)
    write_text(out_dir / f"{archive_name}.sha256", f"{archive_sha}  {archive_name}\n")
    write_text(
        out_dir / f"{workflow_archive_name}.sha256",
        f"{workflow_archive_sha}  {workflow_archive_name}\n",
    )
    write_text(
        out_dir / "SHA256SUMS",
        "\n".join(
            [
                f"{archive_sha}  {archive_name}",
                f"{workflow_archive_sha}  {workflow_archive_name}",
            ]
        )
        + "\n",
    )
    write_text(out_dir / "VERSION", version + "\n")
    copy_file(installer, out_dir / "install.sh", 0o755)

    print(str(out_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

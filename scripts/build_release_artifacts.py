#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import subprocess
import tarfile
from typing import Optional
from hashlib import sha256
from pathlib import Path

import release_archive


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
    parser.add_argument(
        "--signing-key",
        default=os.environ.get("RZN_PHONE_RELEASE_SIGNING_KEY", ""),
        help="Path to a base64 Ed25519 private seed used to sign release tarballs.",
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


def copy_file(src: Path, dest: Path, mode: Optional[int] = None) -> None:
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


def resolve_signing_key(root: Path, explicit: str) -> Path:
    candidates = []
    if explicit:
        candidates.append(Path(explicit).expanduser())
    candidates.append(root / ".secrets" / "plugin-signing" / "ed25519.private")
    for candidate in candidates:
        if candidate.exists():
            return candidate.resolve()
    raise SystemExit(
        "missing release signing key; set RZN_PHONE_RELEASE_SIGNING_KEY or pass --signing-key"
    )


def assert_signing_key_matches_bundled_public(root: Path, signing_key: Path) -> None:
    seed = release_archive.read_base64_bytes(
        signing_key,
        expected_len=32,
        label="Ed25519 private seed",
    )
    expected_public = release_archive.read_base64_bytes(
        root / "scripts" / "rzn_phone_release_ed25519.pub",
        expected_len=32,
        label="bundled Ed25519 public key",
    )
    actual_public = release_archive.ed25519_public_from_seed(seed)
    if actual_public == expected_public:
        return
    if os.environ.get("RZN_PHONE_RELEASE_ALLOW_TEST_SIGNING_KEY") == "1":
        return
    raise SystemExit(
        "release signing key does not match scripts/rzn_phone_release_ed25519.pub"
    )


def write_archive_sidecars(archive_path: Path, signing_key: Path) -> str:
    archive_sha = sha256_file(archive_path)
    write_text(
        archive_path.with_name(f"{archive_path.name}.sha256"),
        f"{archive_sha}  {archive_path.name}\n",
    )
    release_archive.sign_file(
        archive_path,
        signing_key,
        archive_path.with_name(f"{archive_path.name}.sig"),
    )
    return archive_sha


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


def ensure_universal_binaries(root: Path) -> dict[str, Path]:
    out_dir = root / "dist" / "bin" / "macos" / "universal"
    binaries = {
        "cli": out_dir / "rzn-phone",
        "worker": out_dir / "rzn-phone-worker",
    }
    if all(path.exists() for path in binaries.values()):
        return binaries
    subprocess.run([str(root / "scripts" / "build_universal.sh")], check=True)
    missing = [str(path) for path in binaries.values() if not path.exists()]
    if missing:
        raise SystemExit(f"expected universal binaries at {', '.join(missing)}")
    return binaries


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
    signing_key = resolve_signing_key(root, args.signing_key)
    assert_signing_key_matches_bundled_public(root, signing_key)

    package_dir = out_dir / "package"
    workflow_pack_dir = out_dir / "workflow-pack"
    reset_dir(package_dir)
    reset_dir(workflow_pack_dir)

    binaries = ensure_universal_binaries(root)
    workflow_dir = root / "crates" / "rzn_phone_worker" / "resources" / "workflows"
    systems_dir = root / "crates" / "rzn_phone_worker" / "resources" / "systems"
    examples_dir = root / "examples"
    skills_dir = root / "skills"
    license_path = root / "LICENSE"
    installer = root / "scripts" / "install_rzn_phone.sh"

    copy_file(binaries["cli"], package_dir / "bin" / "rzn-phone", 0o755)
    copy_file(binaries["worker"], package_dir / "libexec" / "rzn-phone-worker", 0o755)
    copy_file(license_path, package_dir / "LICENSE")
    copy_tree(workflow_dir, package_dir / "resources" / "workflows")
    copy_tree(systems_dir, package_dir / "resources" / "systems")
    copy_tree(examples_dir, package_dir / "examples")
    copy_tree(skills_dir, package_dir / "skills")
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
    skills = [
        f"skills/{path.relative_to(skills_dir).as_posix()}"
        for path in sorted(skills_dir.rglob("*"))
        if path.is_file()
    ]

    copy_file(license_path, workflow_pack_dir / "LICENSE")
    copy_tree(workflow_dir, workflow_pack_dir / "resources" / "workflows")
    copy_tree(systems_dir, workflow_pack_dir / "resources" / "systems")
    copy_tree(examples_dir, workflow_pack_dir / "examples")
    copy_tree(skills_dir, workflow_pack_dir / "skills")
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
                "skills": skills,
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

    archive_sha = write_archive_sidecars(archive_path, signing_key)
    workflow_archive_sha = write_archive_sidecars(workflow_archive_path, signing_key)
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
    copy_file(root / "scripts" / "release_archive.py", out_dir / "release_archive.py", 0o755)
    copy_file(
        root / "scripts" / "rzn_phone_release_ed25519.pub",
        out_dir / "rzn_phone_release_ed25519.pub",
    )

    print(str(out_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

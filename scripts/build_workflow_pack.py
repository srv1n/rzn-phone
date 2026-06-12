#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import json
import shutil
import tarfile
from hashlib import sha256
from pathlib import Path

import release_archive


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Package workflow/example assets for GitHub releases."
    )
    parser.add_argument(
        "--config",
        default="plugin_bundle/rzn-phone.bundle.json",
        help="Bundle config path used as the metadata source.",
    )
    parser.add_argument(
        "--out",
        default="dist/workflow-packs",
        help="Output root for packaged workflow assets.",
    )
    parser.add_argument(
        "--signing-key",
        default=os.environ.get("RZN_PHONE_RELEASE_SIGNING_KEY", ""),
        help="Optional base64 Ed25519 private seed used to sign the workflow pack.",
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


def write_archive_sidecars(archive_path: Path, signing_key: str) -> str:
    archive_sha = sha256_file(archive_path)
    write_text(
        archive_path.with_name(f"{archive_path.name}.sha256"),
        f"{archive_sha}  {archive_path.name}\n",
    )
    if signing_key:
        release_archive.sign_file(
            archive_path,
            Path(signing_key).expanduser().resolve(),
            archive_path.with_name(f"{archive_path.name}.sig"),
        )
    return archive_sha


def assert_signing_key_matches_bundled_public(root: Path, signing_key: str) -> None:
    if not signing_key:
        return
    key_path = Path(signing_key).expanduser().resolve()
    seed = release_archive.read_base64_bytes(
        key_path,
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


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parent.parent
    config = load_json((root / args.config).resolve())
    plugin_id = str(config["id"]).strip()
    version = str(config["version"]).strip()
    out_dir = (root / args.out / plugin_id / version).resolve()
    pack_dir = out_dir / "package"
    reset_dir(pack_dir)
    assert_signing_key_matches_bundled_public(root, args.signing_key)

    workflow_dir = root / "crates" / "rzn_phone_worker" / "resources" / "workflows"
    systems_dir = root / "crates" / "rzn_phone_worker" / "resources" / "systems"
    examples_dir = root / "examples"

    copy_tree(workflow_dir, pack_dir / "resources" / "workflows")
    copy_tree(systems_dir, pack_dir / "resources" / "systems")
    copy_tree(examples_dir, pack_dir / "examples")

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

    write_text(pack_dir / "VERSION", version + "\n")
    write_text(
        pack_dir / "pack.json",
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

    archive_name = f"{plugin_id}-workflows-{version}.tar.gz"
    archive_path = out_dir / archive_name
    build_archive(pack_dir, archive_path, f"{plugin_id}-workflows")
    archive_sha = write_archive_sidecars(archive_path, args.signing_key)
    write_text(out_dir / "SHA256SUMS", f"{archive_sha}  {archive_name}\n")
    print(str(out_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

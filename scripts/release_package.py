from __future__ import annotations

import gzip
import json
import shutil
import tarfile
from hashlib import sha256
from pathlib import Path


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
    info.uid = info.gid = info.mtime = 0
    info.uname = info.gname = "root"
    return info


def build_tar_gz(source_dir: Path, archive_path: Path, root_name: str) -> None:
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    with archive_path.open("wb") as handle:
        with gzip.GzipFile(filename="", fileobj=handle, mode="wb", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                archive.add(source_dir, arcname=root_name, filter=normalized_tarinfo)


def workflow_metadata(workflow_dir: Path) -> list[dict]:
    return [
        {
            "name": raw.get("name", path.stem),
            "version": raw.get("version", ""),
            "path": f"resources/workflows/{path.name}",
        }
        for path in sorted(workflow_dir.glob("*.json"))
        for raw in [load_json(path)]
    ]


def file_paths(root: Path, prefix: str) -> list[str]:
    return [
        "/".join(part for part in (prefix, path.relative_to(root).as_posix()) if part)
        for path in sorted(root.rglob("*"))
        if path.is_file()
    ]

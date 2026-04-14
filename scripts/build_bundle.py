#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import zipfile
from collections import OrderedDict
from dataclasses import dataclass
from pathlib import Path


FIXED_ZIP_DT = (1980, 1, 1, 0, 0, 0)


@dataclass(frozen=True)
class PayloadFile:
    source: Path
    mode: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build and sign the rzn-phone plugin ZIP.")
    parser.add_argument("--config", required=True, help="Path to bundle config JSON.")
    parser.add_argument("--platform", required=True, help="Target platform key.")
    parser.add_argument("--key", required=True, help="Path to Ed25519 private key (base64 seed).")
    parser.add_argument("--out", default="dist/plugins", help="Output directory.")
    parser.add_argument("--devkit", default="", help="Path to rzn-plugin-devkit binary.")
    return parser.parse_args()


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def expand_env(raw: str) -> str:
    if "$" not in raw:
        return raw
    expanded = os.path.expandvars(raw)
    if "$" in expanded:
        raise ValueError(f"unresolved env var in path: {raw}")
    return expanded


def parse_mode(raw) -> int:
    if isinstance(raw, int):
        return raw
    return int(str(raw), 8)


def normalize_dest(raw: str) -> str:
    return Path(raw).as_posix().lstrip("./")


def collect_payloads(config: dict, platform: str) -> dict[str, PayloadFile]:
    payloads: dict[str, PayloadFile] = {}
    for item in config.get("payloads", []) + config.get("shared_payloads", []):
        item_platforms = item.get("platforms")
        if item_platforms and platform not in item_platforms:
            continue

        source_raw = item.get("source")
        dest_raw = item.get("dest")
        if not source_raw or not dest_raw:
            raise ValueError("payload item requires source and dest")

        source_path = Path(expand_env(str(source_raw))).expanduser().resolve()
        mode = parse_mode(item.get("mode", "644"))
        dest_root = normalize_dest(str(dest_raw))

        if source_path.is_dir():
            for root, _, files in os.walk(source_path):
                for filename in sorted(files):
                    file_path = Path(root) / filename
                    rel = file_path.relative_to(source_path).as_posix()
                    dest = normalize_dest(f"{dest_root}/{rel}")
                    payload_mode = mode if mode != 0o644 else (file_path.stat().st_mode & 0o777)
                    if dest in payloads:
                        raise ValueError(f"duplicate payload destination: {dest}")
                    payloads[dest] = PayloadFile(source=file_path, mode=payload_mode)
        else:
            dest = dest_root
            if dest in payloads:
                raise ValueError(f"duplicate payload destination: {dest}")
            payloads[dest] = PayloadFile(source=source_path, mode=mode)

    if not payloads:
        raise ValueError("no payloads collected for selected platform")
    return payloads


def sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            hasher.update(chunk)
    return hasher.hexdigest()


def build_manifest(config: dict, platform: str, sha_map: OrderedDict) -> OrderedDict:
    manifest = OrderedDict()
    manifest["v"] = 1
    manifest["id"] = config["id"]
    manifest["version"] = config["version"]
    manifest["name"] = config["name"]
    if config.get("description"):
        manifest["description"] = config["description"]
    if config.get("min_host_version"):
        manifest["min_host_version"] = config["min_host_version"]
    if config.get("mcp_protocol_version"):
        manifest["mcp_protocol_version"] = config["mcp_protocol_version"]

    workers = []
    for worker in config.get("workers", []):
        entry = worker.get("entrypoints", {}).get(platform)
        if not entry:
            raise ValueError(f"missing worker entrypoint for platform: {platform}")
        worker_out = OrderedDict()
        worker_out["id"] = worker["id"]
        worker_out["kind"] = worker.get("kind", "mcp_stdio")
        worker_out["auto_start"] = bool(worker.get("auto_start", False))
        worker_out["entrypoint"] = {platform: entry}
        worker_out["args"] = worker.get("args", [])
        worker_out["env"] = worker.get("env", {})
        if worker.get("tools_namespace"):
            worker_out["tools_namespace"] = worker["tools_namespace"]
        workers.append(worker_out)
    manifest["workers"] = workers
    manifest["resources"] = config.get("resources", [])
    manifest["sha256"] = sha_map
    return manifest


def resolve_devkit_bin(explicit: str) -> str:
    if explicit:
        return explicit

    env_override = os.environ.get("RZN_PLUGIN_DEVKIT_BIN", "").strip()
    if env_override:
        return env_override

    candidates = [
        shutil.which("rzn-plugin-devkit"),
        "/Users/sarav/Downloads/side/rzn/rzn-browser-native/target/release/rzn-plugin-devkit",
        "/Users/sarav/Downloads/side/rzn/rzn-python-sandbox/target/release/rzn-plugin-devkit",
    ]
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return str(candidate)
    raise RuntimeError("rzn-plugin-devkit not found")


def sign_manifest(devkit_bin: str, key_path: Path, manifest_path: Path, sig_path: Path) -> None:
    subprocess.run(
        [
            devkit_bin,
            "sign",
            "--key",
            str(key_path),
            "--input",
            str(manifest_path),
            "--output",
            str(sig_path),
        ],
        check=True,
    )


def write_manifest(path: Path, manifest: OrderedDict) -> None:
    content = json.dumps(manifest, separators=(",", ":"), ensure_ascii=True)
    path.write_text(content + "\n", encoding="utf-8")


def write_zip(path: Path, manifest_path: Path, sig_path: Path, payloads: dict[str, PayloadFile]) -> None:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for rel, source, mode in [
            ("plugin.json", manifest_path, 0o644),
            ("plugin.sig", sig_path, 0o644),
        ]:
            info = zipfile.ZipInfo(rel, FIXED_ZIP_DT)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (mode & 0o777) << 16
            archive.writestr(info, source.read_bytes())

        for dest in sorted(payloads.keys()):
            payload = payloads[dest]
            info = zipfile.ZipInfo(dest, FIXED_ZIP_DT)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (payload.mode & 0o777) << 16
            archive.writestr(info, payload.source.read_bytes())


def main() -> int:
    args = parse_args()
    config_path = Path(args.config).resolve()
    config = load_json(config_path)
    platform = args.platform

    if config.get("platforms") and platform not in config["platforms"]:
        raise SystemExit(f"platform '{platform}' is not allowed by config")

    key_path = Path(args.key).expanduser().resolve()
    if not key_path.exists():
        raise SystemExit(f"key not found: {key_path}")

    payloads = collect_payloads(config, platform)
    sha_map = OrderedDict()
    for dest in sorted(payloads.keys()):
        sha_map[dest] = sha256_file(payloads[dest].source)

    resources = config.get("resources", [])
    for resource in resources:
        if isinstance(resource, dict):
            resource_path = resource.get("path", "")
        else:
            resource_path = str(resource)
        if resource_path and resource_path not in sha_map:
            raise SystemExit(f"resource path missing from payloads: {resource_path}")

    out_dir = Path(args.out).resolve()
    stage = out_dir / config["id"] / config["version"] / platform
    stage.mkdir(parents=True, exist_ok=True)

    manifest_path = stage / "plugin.json"
    sig_path = stage / "plugin.sig"
    zip_path = stage / f"{config['id']}-{config['version']}-{platform}.zip"
    sha_path = stage / f"{config['id']}-{config['version']}-{platform}.zip.sha256"

    manifest = build_manifest(config, platform, sha_map)
    write_manifest(manifest_path, manifest)

    devkit_bin = resolve_devkit_bin(args.devkit)
    sign_manifest(devkit_bin, key_path, manifest_path, sig_path)
    write_zip(zip_path, manifest_path, sig_path, payloads)
    sha_path.write_text(f"{sha256_file(zip_path)}  {zip_path.name}\n", encoding="utf-8")

    print(str(zip_path))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

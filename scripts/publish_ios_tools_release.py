#!/usr/bin/env python3
"""
Build + publish the ios-tools plugin bundle to the backend.

Preferred local env (scoped publisher flow):
  - RZN_BACKEND_BASE_URL
  - RZN_PLUGIN_PRODUCT_ID
  - RZN_PUBLISHER_KEY

Legacy fallback env:
  - RZN_PLATFORM_ADMIN_TOKEN
  - R2_PLUGINS_ACCESS_KEY_ID
  - R2_PLUGINS_SECRET_ACCESS_KEY
  - R2_PLUGINS_BUCKET
  - R2_PLUGINS_ENDPOINT
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path


def sh(cmd: list[str], *, env: dict | None = None) -> None:
    subprocess.run(cmd, check=True, env=env)


def sha256_hex(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def http_request_json(
    method: str,
    url: str,
    *,
    headers: dict[str, str] | None = None,
    payload: dict | None = None,
) -> dict:
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, method=method, data=body)
    if payload is not None:
        req.add_header("Content-Type", "application/json")
    for key, value in (headers or {}).items():
        req.add_header(key, value)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
            return json.loads(raw) if raw.strip() else {}
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{method} {url} failed: {e.code} {raw}") from None


def http_post_json(url: str, token: str, payload: dict) -> dict:
    return http_request_json(
        "POST",
        url,
        headers={"Authorization": f"Bearer {token}"},
        payload=payload,
    )


def load_config(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def load_env_file(path: Path) -> None:
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip('"'))


def maybe_load_seeded_publisher_env(root: Path, plugin_id: str) -> None:
    candidates = [
        root.parent / "backend" / ".secrets" / "plugin-publishers" / f"{plugin_id}.env",
        root / ".secrets" / f"plugin-publisher-{plugin_id}.env",
        root / ".secrets" / "plugin-publisher.env",
    ]
    for candidate in candidates:
        load_env_file(candidate)


def upload_presigned(
    upload_url: str, zip_path: Path, *, headers: dict[str, str] | None = None
) -> None:
    req = urllib.request.Request(upload_url, method="PUT", data=zip_path.read_bytes())
    req.add_header("Content-Type", "application/zip")
    for key, value in (headers or {}).items():
        req.add_header(key, value)
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            resp.read()
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"PUT {upload_url} failed: {e.code} {raw}") from None


def aws_env_from_r2() -> dict:
    access_key = os.environ.get("R2_PLUGINS_ACCESS_KEY_ID", "").strip()
    secret_key = os.environ.get("R2_PLUGINS_SECRET_ACCESS_KEY", "").strip()
    region = os.environ.get("R2_PLUGINS_REGION", "auto").strip()
    if not access_key or not secret_key:
        raise RuntimeError("missing R2_PLUGINS_ACCESS_KEY_ID / R2_PLUGINS_SECRET_ACCESS_KEY")
    env = os.environ.copy()
    env["AWS_ACCESS_KEY_ID"] = access_key
    env["AWS_SECRET_ACCESS_KEY"] = secret_key
    env["AWS_DEFAULT_REGION"] = region
    return env


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Build + upload + publish ios-tools to the backend."
    )
    ap.add_argument(
        "--config",
        default="plugin_bundle/ios-tools.bundle.json",
        help="Plugin config JSON path",
    )
    ap.add_argument(
        "--build-script",
        default="scripts/package_plugin.sh",
        help="Build script to run when --skip-build is not set",
    )
    ap.add_argument("--platform", default="macos_universal", help="Platform key")
    ap.add_argument("--channel", default="stable", choices=["stable", "beta", "nightly"])
    ap.add_argument("--skip-build", action="store_true", help="Skip build steps")
    ap.add_argument("--skip-upload", action="store_true", help="Skip artifact upload")
    ap.add_argument("--skip-publish", action="store_true", help="Skip publish step")
    args = ap.parse_args()

    root = Path(__file__).resolve().parents[1]
    config_path = (root / args.config).resolve()
    config = load_config(config_path)

    plugin_id = str(config["id"]).strip()
    version = str(config["version"]).strip()
    maybe_load_seeded_publisher_env(root, plugin_id)

    backend_base = os.environ.get("RZN_BACKEND_BASE_URL", "").strip().rstrip("/")
    admin_token = os.environ.get("RZN_PLATFORM_ADMIN_TOKEN", "").strip()
    product_id = os.environ.get("RZN_PLUGIN_PRODUCT_ID", "").strip()
    publisher_key = os.environ.get("RZN_PUBLISHER_KEY", "").strip()
    if not backend_base:
        raise RuntimeError("missing RZN_BACKEND_BASE_URL (e.g. http://localhost:8082)")

    if not args.skip_build:
        sh(["bash", str((root / args.build_script).resolve())])

    zip_name = f"{plugin_id}-{version}-{args.platform}.zip"
    zip_path = root / "dist" / "plugins" / plugin_id / version / args.platform / zip_name
    if not zip_path.exists():
        raise RuntimeError(f"missing built zip: {zip_path}")

    digest = sha256_hex(zip_path)
    if product_id and publisher_key:
        if args.skip_upload:
            raise RuntimeError("--skip-upload is not supported with the scoped publisher flow")
        headers = {"x-rzn-publisher-key": publisher_key}
        release = http_request_json(
            "POST",
            f"{backend_base}/publisher/products/{product_id}/releases",
            headers=headers,
            payload={"version": version, "platform": args.platform},
        )
        release_data = release.get("data", release)
        release_id = str(release_data["id"]).strip()
        upload = http_request_json(
            "POST",
            f"{backend_base}/publisher/releases/{release_id}/upload-session",
            headers=headers,
        )
        upload_data = upload.get("data", upload)
        upload_url = str(upload_data["upload_url"])
        upload_headers = headers if "/publisher/releases/" in upload_url else None
        upload_presigned(upload_url, zip_path, headers=upload_headers)
        finalized = http_request_json(
            "POST",
            f"{backend_base}/publisher/releases/{release_id}/finalize",
            headers=headers,
            payload={
                "artifact_sha256": digest,
                "release_notes": "phone ios-tools publish",
                "metadata": {"artifact_key": upload_data.get("artifact_key")},
            },
        )
        print("finalized:", finalized)
        if not args.skip_publish:
            published = http_request_json(
                "POST",
                f"{backend_base}/publisher/releases/{release_id}/publish",
                headers=headers,
                payload={"channel": args.channel},
            )
            print("published:", published)
        return 0

    if not admin_token:
        raise RuntimeError(
            "missing scoped publisher env (RZN_PLUGIN_PRODUCT_ID + RZN_PUBLISHER_KEY) and missing "
            "legacy fallback RZN_PLATFORM_ADMIN_TOKEN"
        )

    r2_bucket = os.environ.get("R2_PLUGINS_BUCKET", "").strip()
    r2_endpoint = os.environ.get("R2_PLUGINS_ENDPOINT", "").strip()
    r2_prefix = os.environ.get("R2_PLUGINS_PREFIX", "plugins").strip().strip("/")
    if not r2_bucket:
        raise RuntimeError("missing R2_PLUGINS_BUCKET")
    if not r2_endpoint:
        raise RuntimeError("missing R2_PLUGINS_ENDPOINT")

    artifact_key = f"{r2_prefix}/{plugin_id}/{version}/{args.platform}/{zip_name}"
    if not args.skip_upload:
        env = aws_env_from_r2()
        sh(["aws", "configure", "set", "default.s3.addressing_style", "path"], env=env)
        sh(
            [
                "aws",
                "s3api",
                "put-object",
                "--endpoint-url",
                r2_endpoint,
                "--bucket",
                r2_bucket,
                "--key",
                artifact_key,
                "--body",
                str(zip_path),
                "--content-type",
                "application/zip",
            ],
            env=env,
        )

    registered = http_post_json(
        f"{backend_base}/admin/plugins/releases",
        admin_token,
        {
            "plugin_id": plugin_id,
            "version": version,
            "platform": args.platform,
            "artifact_key": artifact_key,
            "artifact_sha256": digest,
            "notes": "phone ios-tools publish",
        },
    )
    print("registered:", registered)

    if not args.skip_publish:
        published = http_post_json(
            f"{backend_base}/admin/plugins/catalog/publish",
            admin_token,
            {"channel": args.channel, "base_url": f"{backend_base}/plugins/artifacts"},
        )
        print("published:", published)

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"error: {e}", file=sys.stderr)
        raise

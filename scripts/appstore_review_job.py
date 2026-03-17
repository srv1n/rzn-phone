#!/usr/bin/env python3
"""
Run an App Store review job end-to-end.

The job JSON shape follows docs/phone-team-spec.md from the outreach repo:
  - launch installed app (bundle id resolved from app_id when needed)
  - open App Store, draft/submit review through appstore.post_review
  - upload proof screenshots to R2
  - POST success/failure callback
"""

from __future__ import annotations

import argparse
import base64
import json
import json.decoder
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path, PurePosixPath
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
WORKER_BIN = ROOT / "target" / "release" / "rzn_ios_tools_worker"
WORKFLOW_NAME = "appstore.post_review"


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description="Run an App Store review job through ios-tools.")
    ap.add_argument("udid", help="Target device UDID")
    ap.add_argument("job", help="Path to review job JSON")
    ap.add_argument("--out", help="Output directory for raw/result/artifacts")
    ap.add_argument(
        "--dry-run",
        action="store_true",
        help="Draft the review without submitting it. This also skips callback delivery.",
    )
    ap.add_argument(
        "--skip-upload",
        action="store_true",
        help="Do not upload screenshots to R2. Success callbacks require uploads.",
    )
    ap.add_argument(
        "--skip-callback",
        action="store_true",
        help="Do not POST the callback payload.",
    )
    return ap.parse_args()


def bool_env(name: str, default: bool = False) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return default
    return raw.strip().lower() in {"1", "true", "yes", "on"}


def required_string(obj: dict[str, Any], key: str) -> str:
    value = obj.get(key)
    if isinstance(value, str) and value.strip():
        return value.strip()
    raise RuntimeError(f"job field '{key}' is required")


def optional_string(obj: dict[str, Any], key: str) -> str | None:
    value = obj.get(key)
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def http_request_json(
    method: str,
    url: str,
    *,
    headers: dict[str, str] | None = None,
    payload: dict[str, Any] | None = None,
    timeout: int = 60,
) -> dict[str, Any]:
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, method=method, data=body)
    if payload is not None:
        req.add_header("Content-Type", "application/json")
    for key, value in (headers or {}).items():
        req.add_header(key, value)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
            if not raw.strip():
                return {}
            try:
                return json.loads(raw)
            except json.decoder.JSONDecodeError:
                return {"raw": raw}
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{method} {url} failed: {exc.code} {raw}") from None
    except urllib.error.URLError as exc:
        raise RuntimeError(f"{method} {url} failed: {exc}") from None


def parse_country_from_app_url(app_url: str | None) -> str:
    if not app_url:
        return "us"
    match = re.search(r"apps\.apple\.com/([a-zA-Z]{2})(?:/|$)", app_url)
    if match:
        return match.group(1).lower()
    return "us"


def resolve_bundle_id(job: dict[str, Any]) -> str:
    for key in ("installed_app_bundle_id", "app_bundle_id", "bundle_id"):
        if value := optional_string(job, key):
            return value

    app_id = optional_string(job, "app_id")
    if not app_id:
        raise RuntimeError(
            "job must include installed_app_bundle_id/app_bundle_id or resolvable app_id"
        )

    country = parse_country_from_app_url(optional_string(job, "app_url"))
    query = urllib.parse.urlencode({"id": app_id, "country": country})
    payload = http_request_json("GET", f"https://itunes.apple.com/lookup?{query}", timeout=30)
    results = payload.get("results")
    if not isinstance(results, list) or not results:
        raise RuntimeError(f"bundle id lookup returned no results for app_id={app_id}")
    bundle_id = results[0].get("bundleId")
    if not isinstance(bundle_id, str) or not bundle_id.strip():
        raise RuntimeError(f"bundle id lookup missing bundleId for app_id={app_id}")
    return bundle_id.strip()


def resolve_worker_bin() -> Path:
    if bool_env("IOS_TOOLS_SKIP_BUILD", False):
        if not WORKER_BIN.exists():
            raise RuntimeError(f"missing worker binary at {WORKER_BIN}")
        return WORKER_BIN

    if bool_env("IOS_TOOLS_FORCE_BUILD", False) or not WORKER_BIN.exists():
        subprocess.run(
            ["cargo", "build", "-p", "rzn_ios_tools_worker", "--release"],
            cwd=ROOT,
            check=True,
        )
    return WORKER_BIN


def build_session(udid: str) -> dict[str, Any]:
    session: dict[str, Any] = {
        "udid": udid,
        "showXcodeLog": bool_env("IOS_SHOW_XCODE_LOG", False),
        "allowProvisioningUpdates": bool_env("IOS_ALLOW_PROVISIONING_UPDATES", False),
        "allowProvisioningDeviceRegistration": bool_env(
            "IOS_ALLOW_PROVISIONING_DEVICE_REGISTRATION", False
        ),
        "sessionCreateTimeoutMs": int(os.environ.get("IOS_SESSION_CREATE_TIMEOUT_MS", "600000")),
        "wdaLaunchTimeoutMs": int(os.environ.get("IOS_WDA_LAUNCH_TIMEOUT_MS", "240000")),
        "wdaConnectionTimeoutMs": int(
            os.environ.get("IOS_WDA_CONNECTION_TIMEOUT_MS", "120000")
        ),
        "signing": {
            "xcodeOrgId": os.environ.get("IOS_XCODE_ORG_ID", "").strip(),
            "xcodeSigningId": os.environ.get("IOS_XCODE_SIGNING_ID", "").strip(),
            "updatedWDABundleId": os.environ.get("IOS_UPDATED_WDA_BUNDLE_ID", "").strip(),
        },
    }
    wda_local_port = os.environ.get("IOS_WDA_LOCAL_PORT", "").strip()
    if wda_local_port:
        session["wdaLocalPort"] = int(wda_local_port)
    if not any(session["signing"].values()):
        session["signing"] = {}
    return session


def build_shutdown_args() -> dict[str, Any]:
    return {
        "stopAppium": bool_env("IOS_STOP_APPIUM_ON_EXIT", True),
        "shutdownWDA": True,
        "backgroundApp": bool_env("IOS_BACKGROUND_APP_ON_EXIT", False),
        "lockDevice": bool_env("IOS_LOCK_DEVICE_ON_EXIT", False),
    }


def run_workflow(
    *,
    worker_bin: Path,
    session: dict[str, Any],
    workflow_args: dict[str, Any],
    commit: bool,
    out_dir: Path,
) -> dict[str, Any]:
    disconnect_on_finish = bool_env("IOS_WORKFLOW_DISCONNECT_ON_FINISH", True)
    stop_appium_on_finish = bool_env("IOS_WORKFLOW_STOP_APPIUM_ON_FINISH", False)
    background_on_finish = bool_env("IOS_WORKFLOW_BACKGROUND_ON_FINISH", False) or bool_env(
        "IOS_BACKGROUND_APP_ON_EXIT", False
    )
    lock_on_finish = bool_env("IOS_WORKFLOW_LOCK_ON_FINISH", False) or bool_env(
        "IOS_LOCK_DEVICE_ON_EXIT", False
    )
    shutdown_after_run = bool_env("IOS_WORKFLOW_SHUTDOWN_AFTER_RUN", True)

    requests = [
        {
            "jsonrpc": "2.0",
            "id": "init-1",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "appstore-review-job", "version": "0.1"},
            },
        },
        {"jsonrpc": "2.0", "method": "initialized", "params": {}},
        {
            "jsonrpc": "2.0",
            "id": "wf-1",
            "method": "tools/call",
            "params": {
                "name": "ios.workflow.run",
                "arguments": {
                    "name": WORKFLOW_NAME,
                    "session": session,
                    "args": workflow_args,
                    "commit": commit,
                    "disconnectOnFinish": disconnect_on_finish,
                    "closeOnFinish": disconnect_on_finish,
                    "stopAppiumOnFinish": stop_appium_on_finish,
                    "backgroundAppOnFinish": background_on_finish,
                    "lockDeviceOnFinish": lock_on_finish,
                },
            },
        },
    ]
    if shutdown_after_run:
        requests.append(
            {
                "jsonrpc": "2.0",
                "id": "shutdown-1",
                "method": "tools/call",
                "params": {
                    "name": "rzn.worker.shutdown",
                    "arguments": build_shutdown_args(),
                },
            }
        )

    raw_input = "".join(json.dumps(item) + "\n" for item in requests)
    proc = subprocess.run(
        [str(worker_bin)],
        cwd=ROOT,
        input=raw_input.encode("utf-8"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )

    raw_path = out_dir / ".raw.jsonl"
    raw_path.write_bytes(proc.stdout)
    if proc.stderr:
        (out_dir / ".stderr.txt").write_bytes(proc.stderr)
    if proc.returncode != 0:
        raise RuntimeError(
            f"worker exited with code {proc.returncode}: {proc.stderr.decode('utf-8', errors='replace')}"
        )

    responses: list[dict[str, Any]] = []
    for raw_line in proc.stdout.decode("utf-8", errors="replace").splitlines():
        line = raw_line.strip()
        if not line:
            continue
        responses.append(json.loads(line))

    for item in responses:
        if item.get("id") == "wf-1":
            return item
    raise RuntimeError("worker output missing wf-1 response")


def workflow_structured(entry: dict[str, Any]) -> dict[str, Any]:
    result = entry.get("result")
    if isinstance(result, dict):
        structured = result.get("structuredContent")
        if isinstance(structured, dict):
            return structured
    return {}


def workflow_result_is_error(entry: dict[str, Any]) -> bool:
    result = entry.get("result")
    if not isinstance(result, dict):
        return True
    if result.get("isError") is True:
        return True
    structured = workflow_structured(entry)
    return structured.get("ok") is False


def workflow_error_message(entry: dict[str, Any]) -> str:
    result = entry.get("result")
    if isinstance(result, dict):
        structured = workflow_structured(entry)
        if isinstance(structured.get("error"), str) and structured["error"].strip():
            return structured["error"].strip()
        content = result.get("content")
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and isinstance(block.get("text"), str):
                    text = block["text"].strip()
                    if text:
                        return text
    return "workflow failed"


def workflow_failure_artifacts(entry: dict[str, Any]) -> dict[str, Any]:
    result = entry.get("result")
    structured = workflow_structured(entry)
    if isinstance(result, dict) and result.get("isError") is True:
        details = structured.get("details")
        if isinstance(details, dict):
            artifacts = details.get("artifacts")
            if isinstance(artifacts, dict):
                return artifacts
    artifacts = structured.get("artifacts")
    return artifacts if isinstance(artifacts, dict) else {}


def workflow_failed_step_id(entry: dict[str, Any]) -> str | None:
    trace = workflow_structured(entry).get("trace")
    if not isinstance(trace, list):
        return None
    for item in reversed(trace):
        if isinstance(item, dict) and item.get("ok") is False:
            step_id = item.get("stepId")
            if isinstance(step_id, str) and step_id.strip():
                return step_id.strip()
    return None


def classify_workflow_failure(entry: dict[str, Any]) -> str:
    step_id = workflow_failed_step_id(entry)
    base = workflow_error_message(entry)
    if step_id in {"launch_installed_app", "wait_after_app_launch"}:
        return f"App won't launch: {base}"
    if step_id in {"wait_search_results", "tap_target_result_exact", "tap_target_result_fallback"}:
        return f"Target app could not be opened in App Store: {base}"
    if step_id in {"scroll_to_write_review", "tap_write_review"}:
        return "Write a Review button not available"
    if step_id == "wait_submit_confirmation":
        return f"Review submission did not confirm: {base}"
    return base


def relative_output_path(base_dir: Path, relative_key: str) -> Path:
    rel = PurePosixPath(relative_key)
    if rel.is_absolute() or ".." in rel.parts:
        raise RuntimeError(f"unsafe output path: {relative_key}")
    out = base_dir.joinpath(*rel.parts)
    out.parent.mkdir(parents=True, exist_ok=True)
    return out


def save_base64_blob(base_dir: Path, relative_key: str, data: str) -> Path:
    out = relative_output_path(base_dir, relative_key)
    out.write_bytes(base64.b64decode(data))
    return out


def save_text_blob(base_dir: Path, relative_key: str, text: str) -> Path:
    out = relative_output_path(base_dir, relative_key)
    out.write_text(text, encoding="utf-8")
    return out


def upload_keys(upload_path: str) -> dict[str, str]:
    return {
        "app": f"{upload_path}_app_launched.png",
        "draft": f"{upload_path}_review_draft.png",
        "posted": f"{upload_path}_review_posted.png",
        "error": f"{upload_path}_error.png",
    }


def r2_env() -> tuple[dict[str, str], str, str, str]:
    bucket = os.environ.get("OUTREACH_PROOF_R2_BUCKET", "outreach-proof").strip()
    endpoint = os.environ.get("OUTREACH_PROOF_R2_ENDPOINT", "").strip()
    public_base = os.environ.get("OUTREACH_PROOF_R2_PUBLIC_BASE_URL", "").strip().rstrip("/")
    access_key = os.environ.get("OUTREACH_PROOF_R2_ACCESS_KEY_ID", "").strip()
    secret_key = os.environ.get("OUTREACH_PROOF_R2_SECRET_ACCESS_KEY", "").strip()
    region = os.environ.get("OUTREACH_PROOF_R2_REGION", "auto").strip()
    if not endpoint:
        raise RuntimeError("missing OUTREACH_PROOF_R2_ENDPOINT")
    if not public_base:
        raise RuntimeError("missing OUTREACH_PROOF_R2_PUBLIC_BASE_URL")
    if not access_key or not secret_key:
        raise RuntimeError(
            "missing OUTREACH_PROOF_R2_ACCESS_KEY_ID / OUTREACH_PROOF_R2_SECRET_ACCESS_KEY"
        )
    env = os.environ.copy()
    env["AWS_ACCESS_KEY_ID"] = access_key
    env["AWS_SECRET_ACCESS_KEY"] = secret_key
    env["AWS_DEFAULT_REGION"] = region
    return env, bucket, endpoint, public_base


def upload_file_to_r2(local_path: Path, key: str) -> str:
    aws_bin = shutil.which("aws")
    if not aws_bin:
        raise RuntimeError("aws CLI is required for R2 uploads")

    env, bucket, endpoint, public_base = r2_env()
    subprocess.run(
        [
            aws_bin,
            "s3api",
            "put-object",
            "--endpoint-url",
            endpoint,
            "--bucket",
            bucket,
            "--key",
            key,
            "--body",
            str(local_path),
        ],
        check=True,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return f"{public_base}/{urllib.parse.quote(key, safe='/')}"


def normalize_callback_body(job: dict[str, Any]) -> dict[str, Any]:
    value = job.get("callback_body", {})
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise RuntimeError("job field 'callback_body' must be an object")
    return dict(value)


def maybe_post_callback(url: str, payload: dict[str, Any], out_dir: Path) -> None:
    write_json(out_dir / "callback_payload.json", payload)
    response = http_request_json("POST", url, payload=payload, timeout=60)
    write_json(out_dir / "callback_response.json", response)


def build_workflow_args(job: dict[str, Any], bundle_id: str, execute_submit: bool) -> dict[str, Any]:
    args: dict[str, Any] = {}
    overrides = job.get("workflow_args", job.get("workflow_overrides", {}))
    if overrides is not None:
        if not isinstance(overrides, dict):
            raise RuntimeError("job field 'workflow_args' must be an object when present")
        args.update(overrides)

    args.update(
        {
            "installed_app_bundle_id": bundle_id,
            "app_title": required_string(job, "app_title"),
            "review_title": required_string(job, "review_title"),
            "review_body": required_string(job, "review_body"),
            "execute_submit": execute_submit,
        }
    )
    for key in ("app_id", "app_url", "result_index", "submit_mode", "write_review_scrolls"):
        if key in job and job[key] not in (None, ""):
            args[key] = job[key]
    return args


def materialize_success_artifacts(
    entry: dict[str, Any], out_dir: Path, upload_path: str, execute_submit: bool
) -> tuple[dict[str, Path], dict[str, Path]]:
    structured = workflow_structured(entry)
    keys = upload_keys(upload_path)
    screenshots: dict[str, Path] = {}
    aux_files: dict[str, Path] = {}

    app_blob = structured.get("appLaunchedScreenshot")
    draft_blob = structured.get("draftReviewScreenshot")
    posted_blob = structured.get("reviewPostedScreenshot")
    app_source = structured.get("appLaunchedUiSource")
    draft_source = structured.get("draftReviewUiSource")
    posted_source = structured.get("reviewPostedUiSource")

    if not isinstance(app_blob, dict) or not isinstance(app_blob.get("data"), str):
        raise RuntimeError("workflow succeeded without appLaunchedScreenshot")
    if not isinstance(draft_blob, dict) or not isinstance(draft_blob.get("data"), str):
        raise RuntimeError("workflow succeeded without draftReviewScreenshot")

    screenshots["app"] = save_base64_blob(out_dir, keys["app"], app_blob["data"])
    screenshots["draft"] = save_base64_blob(out_dir, keys["draft"], draft_blob["data"])
    if execute_submit:
        if not isinstance(posted_blob, dict) or not isinstance(posted_blob.get("data"), str):
            raise RuntimeError("workflow succeeded without reviewPostedScreenshot")
        screenshots["posted"] = save_base64_blob(out_dir, keys["posted"], posted_blob["data"])

    if isinstance(app_source, dict) and isinstance(app_source.get("source"), str):
        aux_files["app_source"] = save_text_blob(
            out_dir, f"{upload_path}_app_launched.xml", app_source["source"]
        )
    if isinstance(draft_source, dict) and isinstance(draft_source.get("source"), str):
        aux_files["draft_source"] = save_text_blob(
            out_dir, f"{upload_path}_review_draft.xml", draft_source["source"]
        )
    if execute_submit and isinstance(posted_source, dict) and isinstance(
        posted_source.get("source"), str
    ):
        aux_files["posted_source"] = save_text_blob(
            out_dir, f"{upload_path}_review_posted.xml", posted_source["source"]
        )

    write_json(out_dir / "result.json", structured)
    return screenshots, aux_files


def materialize_failure_artifacts(
    entry: dict[str, Any], out_dir: Path, upload_path: str
) -> tuple[Path | None, Path | None]:
    structured = workflow_structured(entry)
    write_json(out_dir / "result.json", structured)
    artifacts = workflow_failure_artifacts(entry)
    screenshot_path: Path | None = None
    source_path: Path | None = None
    screenshot = artifacts.get("screenshot")
    if isinstance(screenshot, dict) and isinstance(screenshot.get("data"), str):
        screenshot_path = save_base64_blob(out_dir, f"{upload_path}_error.png", screenshot["data"])
    ui_source = artifacts.get("uiSource")
    if isinstance(ui_source, dict) and isinstance(ui_source.get("source"), str):
        source_path = save_text_blob(out_dir, f"{upload_path}_error.xml", ui_source["source"])
    return screenshot_path, source_path


def main() -> int:
    args = parse_args()
    job_path = Path(args.job).resolve()
    job = read_json(job_path)

    execute_submit = not args.dry_run
    commit = not args.dry_run
    skip_callback = args.skip_callback or args.dry_run
    upload_path = required_string(job, "upload_path")
    callback_url = optional_string(job, "callback_url")
    callback_body = normalize_callback_body(job)

    if execute_submit and args.skip_upload and not skip_callback:
        raise RuntimeError("cannot send a success callback without uploaded screenshots")
    if not skip_callback and not callback_url:
        raise RuntimeError("job field 'callback_url' is required unless --skip-callback is set")

    safe_job_id = re.sub(r"[^A-Za-z0-9._-]+", "-", str(job.get("job_id", "run"))).strip("-")
    if not safe_job_id:
        safe_job_id = "run"
    out_dir = (
        Path(args.out).resolve()
        if args.out
        else Path(tempfile.mkdtemp(prefix=f"appstore-review-job-{safe_job_id}-"))
    )
    out_dir.mkdir(parents=True, exist_ok=True)
    write_json(out_dir / "job.json", job)

    bundle_id = resolve_bundle_id(job)
    workflow_args = build_workflow_args(job, bundle_id, execute_submit)
    session = build_session(args.udid)
    worker_bin = resolve_worker_bin()
    workflow_entry = run_workflow(
        worker_bin=worker_bin,
        session=session,
        workflow_args=workflow_args,
        commit=commit,
        out_dir=out_dir,
    )
    write_json(out_dir / "workflow_entry.json", workflow_entry)

    callback_payload: dict[str, Any] | None = None
    upload_urls: dict[str, str] = {}

    if not workflow_result_is_error(workflow_entry):
        screenshots, _ = materialize_success_artifacts(
            workflow_entry, out_dir, upload_path, execute_submit
        )
        upload_error: str | None = None
        if not args.skip_upload:
            try:
                for name, path in screenshots.items():
                    upload_urls[name] = upload_file_to_r2(path, upload_keys(upload_path)[name])
            except Exception as exc:
                upload_error = f"R2 upload failed: {exc}"

        if upload_error:
            callback_payload = dict(callback_body)
            callback_payload["error"] = upload_error
            callback_payload["local_artifacts_dir"] = str(out_dir)
            job_result = {
                "ok": False,
                "bundle_id": bundle_id,
                "out_dir": str(out_dir),
                "error": upload_error,
                "upload_urls": upload_urls,
                "callback_payload": callback_payload,
                "submitted": execute_submit,
            }
            write_json(out_dir / "job_result.json", job_result)
            if not skip_callback and callback_url:
                maybe_post_callback(callback_url, callback_payload, out_dir)
            print(json.dumps(job_result))
            return 1

        callback_payload = dict(callback_body)
        if execute_submit:
            if not {"app", "draft", "posted"} <= set(upload_urls):
                if not skip_callback:
                    raise RuntimeError(
                        "success callback requested but not all proof screenshots were uploaded"
                    )
            if upload_urls:
                callback_payload.update(
                    {
                        "screenshot_app": upload_urls["app"],
                        "screenshot_draft": upload_urls["draft"],
                        "screenshot_posted": upload_urls["posted"],
                    }
                )
        else:
            callback_payload["dry_run"] = True
            if upload_urls:
                callback_payload.update(
                    {
                        "screenshot_app": upload_urls["app"],
                        "screenshot_draft": upload_urls["draft"],
                    }
                )

        job_result = {
            "ok": True,
            "bundle_id": bundle_id,
            "out_dir": str(out_dir),
            "upload_urls": upload_urls,
            "callback_payload": callback_payload,
            "submitted": execute_submit,
        }
        write_json(out_dir / "job_result.json", job_result)

        if not skip_callback and callback_url:
            maybe_post_callback(callback_url, callback_payload, out_dir)

        print(
            json.dumps(
                {
                    "ok": True,
                    "submitted": execute_submit,
                    "out_dir": str(out_dir),
                    "upload_urls": upload_urls,
                }
            )
        )
        return 0

    error_message = classify_workflow_failure(workflow_entry)
    error_screenshot, _ = materialize_failure_artifacts(workflow_entry, out_dir, upload_path)
    upload_error: str | None = None
    if error_screenshot and not args.skip_upload:
        try:
            upload_urls["error"] = upload_file_to_r2(
                error_screenshot, upload_keys(upload_path)["error"]
            )
        except Exception as exc:
            upload_error = f"R2 upload failed: {exc}"

    callback_payload = dict(callback_body)
    callback_payload["error"] = (
        f"{error_message}; {upload_error}" if upload_error else error_message
    )
    if upload_urls.get("error"):
        callback_payload["screenshot_error"] = upload_urls["error"]
    callback_payload["local_artifacts_dir"] = str(out_dir)

    job_result = {
        "ok": False,
        "bundle_id": bundle_id,
        "out_dir": str(out_dir),
        "error": callback_payload["error"],
        "failed_step_id": workflow_failed_step_id(workflow_entry),
        "upload_urls": upload_urls,
        "callback_payload": callback_payload,
    }
    write_json(out_dir / "job_result.json", job_result)

    if not skip_callback and callback_url:
        maybe_post_callback(callback_url, callback_payload, out_dir)

    print(json.dumps(job_result))
    return 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # pragma: no cover - CLI fallback
        print(str(exc), file=sys.stderr)
        sys.exit(1)

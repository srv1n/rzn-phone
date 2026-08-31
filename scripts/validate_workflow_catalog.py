#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_DIR = ROOT / "crates" / "rzn_phone_worker" / "resources" / "workflows"
SCHEMA_PATH = ROOT / "schema" / "rzn-mobile-workflow.schema.json"
BUNDLE_CONFIG = ROOT / "plugin_bundle" / "rzn-phone.bundle.json"
CARGO_TOML = ROOT / "crates" / "rzn_phone_worker" / "Cargo.toml"
TOOLS_RS = ROOT / "crates" / "rzn_phone_worker" / "src" / "tools.rs"
TOOLS_DIR = ROOT / "crates" / "rzn_phone_worker" / "src" / "tools"
DEFAULT_OUTPUT_DIR = ROOT / ".tmp" / "rzn-phone-workflow-validation"
DEFAULT_RUNNER = ROOT / "target" / "release" / "rzn-phone"
RUNNER = os.environ.get(
    "RZN_PHONE_BIN",
    str(DEFAULT_RUNNER if DEFAULT_RUNNER.exists() else "rzn-phone"),
)
EXTERNAL_APPIUM_URL = os.environ.get("RZN_IOS_APPIUM_URL", "").strip()
DEFAULT_RUNTIME_STATE_FILE = ROOT / ".tmp" / "runtime-state.json"

APP_ORDER = [
    "safari",
    "appstore",
    "google_maps",
    "phone_messages",
    "reddit",
    "linkedin",
    "instagram",
    "x",
]

STATIC_VALUES = {
    "query": "best headphones 2026",
    "search_query": "reddit",
    "username": "openai",
    "comment_text": "Dry run validation comment.",
    "reply_text": "Dry run validation reply.",
    "message_text": "Dry run validation message.",
    "post_text": "Dry run validation post.",
    "updated_text": "Dry run validation update.",
    "review_title": "Validation Draft",
    "review_body": "Dry run validation review body. Do not submit.",
    "app_title": "Reddit",
    "target_app_name": "Reddit",
    "installed_app_bundle_id": "com.reddit.Reddit",
    "app_id": "1064216828",
    "app_url": "https://apps.apple.com/us/app/reddit/id1064216828",
    "threadContains": "OpenAI",
    "senderContains": "OpenAI",
    "messageContains": "code",
}

FALSE_FLAGS = {
    "execute_comment",
    "execute_delete",
    "execute_like",
    "execute_post",
    "execute_reply",
    "execute_send",
    "execute_start",
    "execute_submit",
    "execute_update",
    "submit",
}

INT_DEFAULTS = {
    "limit": 3,
    "max_posts": 3,
    "max_scrolls": 2,
    "maxScrolls": 2,
    "max_comment_scrolls": 1,
    "max_thread_scrolls": 1,
    "max_feed_scrolls": 1,
    "max_profile_scrolls": 1,
    "max_visible_threads": 3,
    "max_thread_rows": 6,
    "max_comment_rows": 6,
    "maxMessages": 6,
    "maxThreads": 4,
    "maxNodes": 180,
    "post_index": 0,
    "result_index": 0,
    "thread_index": 0,
    "reply_index": 0,
    "post_position": 1,
    "thread_position": 1,
    "write_review_scrolls": 1,
    "max_dwell_ms": 400,
    "min_dwell_ms": 100,
}

ALLOWED_CAPABILITY_FAMILIES = {
    "observe",
    "navigate",
    "extract",
    "interact",
    "verify",
    "session",
    "workflow",
    "utility",
}

RUNTIME_VARS = {
    "udid",
    "bundleId",
    "bundle_id",
    "platformName",
    "showXcodeLog",
    "allowProvisioningUpdates",
    "allowProvisioningDeviceRegistration",
    "sessionCreateTimeoutMs",
    "wdaLocalPort",
    "wdaLaunchTimeoutMs",
    "wdaConnectionTimeoutMs",
    "xcodeOrgId",
    "xcodeSigningId",
    "updatedWDABundleId",
}

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")
TEMPLATE_RE = re.compile(r"\{\{\s*([^{}]+?)\s*\}\}")


def clamp_integer(value: int, spec: dict) -> int:
    minimum = spec.get("min")
    maximum = spec.get("max")
    if isinstance(minimum, int) and value < minimum:
        value = minimum
    if isinstance(maximum, int) and value > maximum:
        value = maximum
    return value


def workflow_sort_key(item):
    data = item[1]
    name = data["name"]
    system = canonical_workflow_id(name).split("/", 1)[0]
    order = APP_ORDER.index(system) if system in APP_ORDER else len(APP_ORDER)
    return (order, name)


def build_args(workflow: dict) -> dict:
    inputs = workflow.get("inputs", {})
    name = workflow["name"]
    system = canonical_workflow_id(name).split("/", 1)[0]
    args = {}

    if system == "google_maps":
        args["query"] = "Starbucks"
    elif system == "appstore":
        if "query" in inputs:
            args["query"] = "reddit"
    elif system == "safari":
        args["query"] = "best headphones 2026"

    for key, spec in inputs.items():
        if key in args:
            continue
        if key in STATIC_VALUES:
            args[key] = STATIC_VALUES[key]
            continue
        if key in FALSE_FLAGS:
            args[key] = False
            continue
        if key in INT_DEFAULTS:
            args[key] = clamp_integer(INT_DEFAULTS[key], spec)
            continue
        if key.startswith("max_") or key.startswith("max") or key.endswith("_limit"):
            args[key] = clamp_integer(3, spec)
            continue

        default = spec.get("default")
        if default is not None:
            continue

        if spec.get("type") == "boolean":
            args[key] = False
        elif spec.get("type") == "integer":
            args[key] = clamp_integer(0, spec)
        elif spec.get("type") == "array":
            args[key] = []
        elif spec.get("type") == "string":
            if key == "country":
                args[key] = "us"
            elif key == "locale":
                args[key] = "en_US"
            elif key == "submit_mode":
                args[key] = "keyboard"
            elif key == "typing_mode":
                args[key] = "full"
            elif key == "review_sort":
                args[key] = "most_helpful"
            elif "contains" in key.lower():
                continue
            else:
                args[key] = f"dry-run-{key}"
    return args


def parse_workflows() -> list[tuple[Path, dict]]:
    flows = []
    for path in sorted(WORKFLOW_DIR.glob("*.json")):
        data = json.loads(path.read_text())
        flows.append((path, data))
    return sorted(flows, key=workflow_sort_key)


def canonical_workflow_id(name: str) -> str:
    raw = name.strip().replace("\\", "/")
    if "/" in raw:
        system, workflow = raw.split("/", 1)
    elif "." in raw:
        system, workflow = raw.split(".", 1)
    else:
        return raw
    return f"{system.strip(' /.')}/{workflow.strip(' /.')}"


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def load_tool_names() -> set[str]:
    raw_parts = []
    if TOOLS_RS.exists():
        raw_parts.append(TOOLS_RS.read_text(encoding="utf-8"))
    if TOOLS_DIR.exists():
        raw_parts.extend(
            path.read_text(encoding="utf-8")
            for path in sorted(TOOLS_DIR.glob("*.rs"))
        )
    return set(re.findall(r'tool\(\s*"([^"]+)"', "\n".join(raw_parts)))


def load_cargo_version() -> str:
    raw = CARGO_TOML.read_text(encoding="utf-8")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', raw)
    if not match:
        raise RuntimeError(f"missing package version in {CARGO_TOML}")
    return match.group(1)


def iter_template_refs(value):
    if isinstance(value, str):
        yield from (match.group(1).strip() for match in TEMPLATE_RE.finditer(value))
    elif isinstance(value, list):
        for item in value:
            yield from iter_template_refs(item)
    elif isinstance(value, dict):
        for item in value.values():
            yield from iter_template_refs(item)


def template_root(ref: str) -> str:
    return ref.split(".", 1)[0].split("[", 1)[0].strip()


def validate_schema_contract(errors: list[str]) -> None:
    schema = read_json(SCHEMA_PATH)
    properties = schema.get("properties", {})
    for key in ["capability", "output", "presentation"]:
        if key not in properties:
            errors.append(f"schema missing top-level property: {key}")

    when = schema.get("$defs", {}).get("when", {})
    when_text = json.dumps(when, sort_keys=True)
    for key in ["var", "equals", "notEquals", "exists", "truthy"]:
        if key not in when_text:
            errors.append(f"schema when clause missing support for: {key}")


def validate_json_schema(workflows: list[tuple[Path, dict]], errors: list[str]) -> None:
    try:
        import jsonschema
    except ImportError:
        errors.append("python package 'jsonschema' is required for offline schema validation")
        return

    schema = read_json(SCHEMA_PATH)
    try:
        validator = jsonschema.Draft202012Validator(schema)
        validator.check_schema(schema)
    except jsonschema.SchemaError as err:
        errors.append(f"workflow schema is invalid: {err.message}")
        return

    for path, workflow in workflows:
        for err in sorted(validator.iter_errors(workflow), key=lambda item: list(item.path)):
            location = ".".join(str(part) for part in err.path) or "<root>"
            errors.append(f"{path.name}: schema violation at {location}: {err.message}")


def validate_workflow_shape(path: Path, workflow: dict, known_tools: set[str], errors: list[str]) -> None:
    label = path.name

    name = workflow.get("name")
    if not isinstance(name, str) or not name.strip():
        errors.append(f"{label}: name must be a non-empty string")
    if not isinstance(workflow.get("version"), str) or not workflow["version"].strip():
        errors.append(f"{label}: version must be a non-empty string")

    inputs = workflow.get("inputs", {})
    if not isinstance(inputs, dict):
        errors.append(f"{label}: inputs must be an object")
        inputs = {}

    required = workflow.get("required_variables", [])
    if not isinstance(required, list) or not all(isinstance(item, str) for item in required):
        errors.append(f"{label}: required_variables must be an array of strings")
        required = []
    for item in required:
        if item not in inputs:
            errors.append(f"{label}: required variable '{item}' is not declared in inputs")

    capability = workflow.get("capability")
    if not isinstance(capability, dict):
        errors.append(f"{label}: capability must be an object")
    else:
        family = capability.get("family")
        if family not in ALLOWED_CAPABILITY_FAMILIES:
            errors.append(f"{label}: invalid capability.family {family!r}")
        for key in ["intent", "surface"]:
            value = capability.get(key)
            if not isinstance(value, str) or not value.strip():
                errors.append(f"{label}: capability.{key} must be a non-empty string")
        if not isinstance(capability.get("mutating"), bool):
            errors.append(f"{label}: capability.mutating must be a boolean")

    for key in ["output", "presentation"]:
        if key in workflow and not isinstance(workflow[key], dict):
            errors.append(f"{label}: {key} must be an object")

    steps = workflow.get("steps")
    if not isinstance(steps, list):
        errors.append(f"{label}: steps must be an array")
        return

    saved_outputs: set[str] = set()
    known_vars = set(inputs) | set(required) | RUNTIME_VARS
    for idx, step in enumerate(steps, start=1):
        if not isinstance(step, dict):
            errors.append(f"{label}: step {idx} must be an object")
            continue
        tool = step.get("tool")
        if not isinstance(tool, str) or not tool.strip():
            errors.append(f"{label}: step {idx} missing tool")
        elif tool not in known_tools:
            errors.append(f"{label}: step {idx} uses unknown tool {tool!r}")

        when = step.get("when")
        if when is not None:
            if isinstance(when, dict):
                condition_keys = [key for key in ["equals", "notEquals", "exists", "truthy"] if key in when]
                if not isinstance(when.get("var"), str) or not when["var"].strip():
                    errors.append(f"{label}: step {idx} when.var must be a non-empty string")
                if len(condition_keys) != 1:
                    errors.append(f"{label}: step {idx} when must set exactly one condition")
            elif not isinstance(when, (bool, str)):
                errors.append(f"{label}: step {idx} when must be a bool, string, or object")

        for ref in iter_template_refs(step):
            root = template_root(ref)
            if root == "steps":
                parts = ref.split(".")
                if len(parts) < 2 or parts[1] not in saved_outputs:
                    errors.append(f"{label}: step {idx} references unresolved template {{{{{ref}}}}}")
            elif root not in known_vars:
                errors.append(f"{label}: step {idx} references undeclared template {{{{{ref}}}}}")

        save_as = step.get("saveAs")
        if isinstance(save_as, str) and save_as.strip():
            saved_outputs.add(save_as.strip())

    known_outputs = known_vars | {"steps"}
    for ref in list(iter_template_refs(workflow.get("output", {}))) + list(
        iter_template_refs(workflow.get("presentation", {}))
    ):
        root = template_root(ref)
        if root == "steps":
            parts = ref.split(".")
            if len(parts) < 2 or parts[1] not in saved_outputs:
                errors.append(f"{label}: output references unresolved template {{{{{ref}}}}}")
        elif root not in known_outputs:
            errors.append(f"{label}: output references undeclared template {{{{{ref}}}}}")


def validate_catalog_metadata(workflows: list[tuple[Path, dict]], errors: list[str]) -> None:
    seen: dict[str, str] = {}
    for path, workflow in workflows:
        workflow_id = canonical_workflow_id(str(workflow.get("name", "")))
        if not workflow_id:
            continue
        if workflow_id in seen:
            errors.append(f"duplicate workflow id {workflow_id!r}: {seen[workflow_id]} and {path.name}")
        seen[workflow_id] = path.name

    config = read_json(BUNDLE_CONFIG)
    version = str(config.get("version", "")).strip()
    cargo_version = load_cargo_version()
    if not SEMVER_RE.match(version):
        errors.append(f"bundle version is not strict semver: {version!r}")
    if version != cargo_version:
        errors.append(f"bundle version {version!r} does not match Cargo version {cargo_version!r}")

    payload_paths = {
        str(item.get("dest", "")).strip()
        for item in config.get("payloads", []) + config.get("shared_payloads", [])
        if isinstance(item, dict)
    }
    expected = {"resources/workflows", "resources/systems"}
    missing = sorted(expected - payload_paths)
    for path in missing:
        errors.append(f"bundle payloads missing {path}")


def validate_offline(json_output: bool = False) -> int:
    errors: list[str] = []
    validate_schema_contract(errors)

    workflows: list[tuple[Path, dict]] = []
    for path in sorted(WORKFLOW_DIR.glob("*.json")):
        try:
            workflows.append((path, read_json(path)))
        except json.JSONDecodeError as err:
            errors.append(f"{path.name}: invalid JSON: {err}")

    validate_json_schema(workflows, errors)

    known_tools = load_tool_names()
    for path, workflow in workflows:
        validate_workflow_shape(path, workflow, known_tools, errors)
    validate_catalog_metadata(workflows, errors)

    summary = {
        "ok": not errors,
        "workflowCount": len(workflows),
        "toolCount": len(known_tools),
        "errors": errors,
    }
    if json_output:
        print(json.dumps(summary, indent=2, sort_keys=True))
    else:
        if errors:
            for error in errors:
                print(f"[FAIL] {error}")
        print(
            json.dumps(
                {
                    "ok": summary["ok"],
                    "workflowCount": summary["workflowCount"],
                    "toolCount": summary["toolCount"],
                    "errorCount": len(errors),
                },
                sort_keys=True,
            )
        )
    return 0 if not errors else 1


def summarize_output(payload: dict) -> str:
    if not isinstance(payload, dict):
        return "non-object output"
    if "resultCount" in payload:
        return f"resultCount={payload.get('resultCount')}"
    if "count" in payload:
        return f"count={payload.get('count')}"
    if "results" in payload and isinstance(payload["results"], list):
        return f"results={len(payload['results'])}"
    if "posts" in payload and isinstance(payload["posts"], list):
        return f"posts={len(payload['posts'])}"
    if "reviews" in payload and isinstance(payload["reviews"], list):
        return f"reviews={len(payload['reviews'])}"
    if "threads" in payload and isinstance(payload["threads"], list):
        return f"threads={len(payload['threads'])}"
    if "suggestions" in payload and isinstance(payload["suggestions"], list):
        return f"suggestions={len(payload['suggestions'])}"
    keys = [key for key in payload.keys() if key not in {"_presentation", "timings", "trace"}]
    return f"keys={','.join(keys[:6])}" if keys else "ok"


def classify_success_output(payload: dict) -> tuple[str, str] | None:
    if not isinstance(payload, dict):
        return None

    passcode_gate_count = payload.get("passcodeGateCount")
    if isinstance(passcode_gate_count, int) and passcode_gate_count > 0:
        return "BLOCKED", "auth_or_onboarding: x_chat_passcode_gate"

    passcode_header_texts = payload.get("passcodeHeaderTexts")
    if isinstance(passcode_header_texts, list) and passcode_header_texts:
        return "BLOCKED", "auth_or_onboarding: x_chat_passcode_gate"

    return None


def classify_failure(stdout: str, stderr: str, payload: dict | None) -> tuple[str, str]:
    text = "\n".join(filter(None, [stdout, stderr]))
    lower = text.lower()
    if payload:
        details = json.dumps(payload).lower()
        lower = f"{lower}\n{details}"

    blocked_patterns = {
        "device_locked": [
            "device_locked",
            "could not be unlocked",
            "the device was not, or could not be, unlocked",
            "unlock",
        ],
        "device_transport": [
            "device transport unavailable",
            "xctrace reports offline",
            "xctrace reports missing",
            "unknown device or simulator udid",
            "could not find the expected device",
            "failed to receive any data within the timeout: 5000",
            "connection invalidated",
            "communicating with a remote process",
        ],
        "wda_build": [
            "xcodebuild failed with code 65",
            "unable to launch webdriveragent",
            "webdriveragent. original error: xcodebuild failed with code 65",
        ],
        "app_missing": ["application is not installed", "failed to launch app", "bundle id"],
        "auth_or_onboarding": ["passcode onboarding", "sign in", "login", "not authenticated", "onboarding", "log in"],
        "permissions": [
            "permission",
            "not allowed",
            "access denied",
            "not authorized for performing ui testing actions",
        ],
        "precondition": [
            "has no posts to update yet",
            "has no posts to delete yet",
        ],
    }
    for reason, patterns in blocked_patterns.items():
        if any(pattern in lower for pattern in patterns):
            return "BLOCKED", reason

    if "timed out" in lower or "timeout" in lower:
        return "FAIL", "timeout"
    if "element_not_found" in lower or "element not found" in lower:
        return "FAIL", "selector"
    if "invalid input" in lower or "required" in lower:
        return "FAIL", "validator"
    return "FAIL", "runtime"


def run_command(command: list[str], timeout_sec: int) -> tuple[subprocess.CompletedProcess[str], bool]:
    env = os.environ.copy()
    env.setdefault("RZN_IOS_RUNTIME_STATE_FILE", str(DEFAULT_RUNTIME_STATE_FILE))
    env.setdefault("RZN_IOS_SMART_CACHE", "0")
    process = subprocess.Popen(
        command,
        cwd=ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout_sec)
        completed = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
        return completed, False
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            stdout, stderr = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = process.communicate()
        completed = subprocess.CompletedProcess(command, 124, stdout or "", stderr or "")
        return completed, True


def safe_name(name: str) -> str:
    return name.replace("/", "_").replace(".", "_")


def shutdown_runtime() -> None:
    if EXTERNAL_APPIUM_URL:
        return
    subprocess.run(
        [RUNNER, "shutdown"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=45,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate the repository rzn-phone workflow catalog.")
    parser.add_argument("--offline", action="store_true", help="Run static schema/catalog validation without a device.")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable validation output.")
    parser.add_argument("--udid", help="Physical iPhone UDID to target.")
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Directory for raw run artifacts.")
    parser.add_argument("--timeout-sec", type=int, default=180, help="Per-workflow timeout in seconds.")
    parser.add_argument("--only", nargs="*", default=[], help="Optional workflow refs to run.")
    parser.add_argument("--skip-shutdown", action="store_true", help="Do not reset runtime between app groups.")
    args = parser.parse_args()

    if args.offline:
        return validate_offline(json_output=args.json)

    if not args.udid:
        parser.error("--udid is required unless --offline is set")

    output_dir = Path(args.output_dir).expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    selected = {value.strip() for value in args.only if value.strip()}
    results = []
    current_system = None

    for path, workflow in parse_workflows():
        workflow_name = workflow["name"]
        canonical = canonical_workflow_id(workflow_name)
        if selected and workflow_name not in selected and canonical not in selected:
            continue

        system = canonical.split("/", 1)[0]
        if current_system != system and current_system is not None and not args.skip_shutdown:
            shutdown_runtime()
        current_system = system

        payload_args = build_args(workflow)
        command = [
            RUNNER,
            "run",
            canonical,
            "--udid",
            args.udid,
            "--args-json",
            json.dumps(payload_args),
            "--json",
        ]

        started = time.time()
        artifact_base = output_dir / safe_name(canonical)
        payload = None
        completed, timed_out = run_command(command, timeout_sec=args.timeout_sec)
        duration_ms = int((time.time() - started) * 1000)
        stdout = completed.stdout
        stderr = completed.stderr
        if stdout.strip():
            try:
                payload = json.loads(stdout)
            except json.JSONDecodeError:
                payload = None

        payload_failed = isinstance(payload, dict) and payload.get("ok") is False

        if timed_out:
            status, reason = "FAIL", "timeout"
        elif completed.returncode == 0 and not payload_failed:
            success_status = classify_success_output(payload if isinstance(payload, dict) else {})
            if success_status is not None:
                status, reason = success_status
            else:
                status, reason = "PASS", summarize_output(payload if isinstance(payload, dict) else {})
        else:
            status, reason = classify_failure(stdout, stderr, payload)

        artifact_base.with_suffix(".args.json").write_text(json.dumps(payload_args, indent=2))
        artifact_base.with_suffix(".stdout.txt").write_text(stdout or "")
        artifact_base.with_suffix(".stderr.txt").write_text(stderr or "")
        if payload is not None:
            artifact_base.with_suffix(".json").write_text(json.dumps(payload, indent=2))

        row = {
            "workflow": canonical,
            "status": status,
            "reason": reason,
            "durationMs": duration_ms,
            "args": payload_args,
            "returnCode": completed.returncode,
            "artifacts": {
                "stdout": str(artifact_base.with_suffix(".stdout.txt")),
                "stderr": str(artifact_base.with_suffix(".stderr.txt")),
                "payload": str(artifact_base.with_suffix(".json")) if payload is not None else None,
            },
        }
        results.append(row)
        print(f"[{status}] {canonical} {duration_ms}ms - {reason}", flush=True)

    summary = {
        "udid": args.udid,
        "generatedAtEpoch": int(time.time()),
        "results": results,
    }
    summary_path = output_dir / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2))

    counts = {}
    for row in results:
        counts[row["status"]] = counts.get(row["status"], 0) + 1
    print(json.dumps({"summary": counts, "summaryPath": str(summary_path)}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())

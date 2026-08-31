#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import select
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import validate_workflow_catalog as catalog  # noqa: E402


DEFAULT_OUTPUT_DIR = ROOT / ".tmp" / "rzn-phone-family-validation"
DEFAULT_STATE_FILE = ROOT / ".tmp" / "runtime-state.json"
WORKER_BIN = ROOT / "target" / "release" / "rzn-phone-worker"
WDA_LOG_ROOT = (
    Path.home()
    / ".appium"
    / "node_modules"
    / "appium-xcuitest-driver"
    / "node_modules"
    / "Logs"
    / "Test"
)


def safe_name(name: str) -> str:
    return name.replace("/", "_").replace(".", "_")


def workflow_bundle_id(workflow: dict) -> str | None:
    for step in workflow.get("steps", []):
        if step.get("tool") != "ios.session.create":
            continue
        bundle_id = step.get("arguments", {}).get("bundleId")
        if isinstance(bundle_id, str) and bundle_id and "{{" not in bundle_id:
            return bundle_id
    return None


def env_flag(name: str, default: bool = False) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return default
    return raw.strip().lower() in {"1", "true", "yes", "on"}


def latest_wda_diagnostics() -> dict | None:
    if not WDA_LOG_ROOT.exists():
        return None

    bundles = sorted(
        WDA_LOG_ROOT.glob("Test-WebDriverAgentRunner-*.xcresult"),
        key=lambda path: path.stat().st_mtime,
        reverse=True,
    )
    if not bundles:
        return None

    bundle = bundles[0]
    scheduling_logs = sorted(bundle.glob("**/scheduling.log"))
    testmanager_logs = sorted(bundle.glob("**/testmanagerd.log"))

    def read_text(path: Path | None) -> str:
        if not path or not path.exists():
            return ""
        try:
            return path.read_text(errors="replace")
        except OSError:
            return ""

    scheduling_text = read_text(scheduling_logs[0] if scheduling_logs else None)
    testmanager_text = read_text(testmanager_logs[0] if testmanager_logs else None)

    clue = ""
    for text in (scheduling_text, testmanager_text):
        for line in text.splitlines():
            lower = line.lower()
            if "timed out while enabling automation mode" in lower:
                clue = line.strip()
                break
            if "failed to initialize for ui testing" in lower:
                clue = line.strip()
                break
        if clue:
            break

    return {
        "bundle": str(bundle),
        "schedulingLog": str(scheduling_logs[0]) if scheduling_logs else None,
        "testmanagerLog": str(testmanager_logs[0]) if testmanager_logs else None,
        "clue": clue or None,
    }


def run_capture(command: list[str], timeout_sec: int = 20) -> dict:
    try:
        proc = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout_sec,
            check=False,
        )
        return {
            "command": command,
            "returncode": proc.returncode,
            "stdout": proc.stdout,
            "stderr": proc.stderr,
            "timedOut": False,
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "command": command,
            "returncode": None,
            "stdout": exc.stdout or "",
            "stderr": exc.stderr or "",
            "timedOut": True,
        }


def probe_xctrace_device_state(udid: str) -> dict:
    capture = run_capture(["xcrun", "xctrace", "list", "devices"])
    section = ""
    matched_line = None
    matched_section = None
    for raw_line in capture.get("stdout", "").splitlines():
        line = raw_line.strip()
        if line.startswith("== ") and line.endswith(" =="):
            section = line
            continue
        if f"({udid})" not in raw_line:
            continue
        matched_line = line
        matched_section = section or "unknown"
        break

    if capture.get("timedOut"):
        status = "probe_timeout"
    elif capture.get("returncode") not in (0, None):
        status = "probe_error"
    elif matched_section == "== Devices ==":
        status = "available"
    elif matched_section == "== Devices Offline ==":
        status = "offline"
    else:
        status = "missing"

    return {
        "status": status,
        "matchedLine": matched_line,
        "matchedSection": matched_section,
        "capture": capture,
    }


class WorkerClient:
    def __init__(self, env: dict[str, str]):
        self.env = env
        self.proc: subprocess.Popen[str] | None = None
        self._start()

    def _start(self) -> None:
        self.proc = subprocess.Popen(
            [str(WORKER_BIN)],
            cwd=ROOT,
            env=self.env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self._send(
            {
                "jsonrpc": "2.0",
                "id": "init-1",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "family-validator", "version": "0.1"},
                },
            }
        )
        self._send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
        self._wait_for({"init-1"}, timeout_sec=20)

    def _send(self, payload: dict) -> None:
        assert self.proc and self.proc.stdin
        self.proc.stdin.write(json.dumps(payload) + "\n")
        self.proc.stdin.flush()

    def _wait_for(self, ids: set[str], timeout_sec: int) -> tuple[dict[str, dict], str]:
        assert self.proc and self.proc.stdout and self.proc.stderr
        seen: dict[str, dict] = {}
        stderr_lines: list[str] = []
        deadline = time.time() + timeout_sec
        while time.time() < deadline and ids - set(seen):
            ready, _, _ = select.select([self.proc.stdout, self.proc.stderr], [], [], 1.0)
            for stream in ready:
                line = stream.readline()
                if not line:
                    continue
                if stream is self.proc.stderr:
                    stderr_lines.append(line)
                    continue
                try:
                    message = json.loads(line)
                except json.JSONDecodeError:
                    continue
                message_id = message.get("id")
                if message_id in ids:
                    seen[message_id] = message
            if self.proc.poll() is not None and not ready:
                break
        return seen, "".join(stderr_lines)

    def call(self, request_id: str, tool_name: str, arguments: dict, timeout_sec: int) -> tuple[dict | None, str]:
        self._send(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "tools/call",
                "params": {"name": tool_name, "arguments": arguments},
            }
        )
        seen, stderr = self._wait_for({request_id}, timeout_sec=timeout_sec)
        return seen.get(request_id), stderr

    def restart(self) -> None:
        self.close()
        self._start()

    def close(self) -> None:
        if not self.proc:
            return
        try:
            self.proc.terminate()
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        self.proc = None


def prewarm_session(
    client: WorkerClient,
    udid: str,
    bundle_id: str | None,
    show_xcode_log: bool,
    allow_provisioning_updates: bool,
    allow_provisioning_device_registration: bool,
) -> tuple[bool, dict | None, str, str]:
    if not bundle_id:
        return True, None, "", ""
    msg, stderr = client.call(
        "prewarm-1",
        "ios.session.create",
        {
            "udid": udid,
            "kind": "native_app",
            "bundleId": bundle_id,
            "reuseActiveSession": True,
            "replaceExisting": False,
            "noReset": True,
            "showXcodeLog": show_xcode_log,
            "allowProvisioningUpdates": allow_provisioning_updates,
            "allowProvisioningDeviceRegistration": allow_provisioning_device_registration,
        },
        timeout_sec=180,
    )
    if not msg:
        return False, None, stderr, "worker_timeout"

    result, structured = summarize_result(msg)
    if structured and structured.get("ok") is True:
        return True, msg, stderr, "session_ready"

    payload = structured if isinstance(structured, dict) else result if isinstance(result, dict) else None
    stdout = json.dumps(payload or {}, indent=2)
    status, reason = catalog.classify_failure(stdout, stderr, payload)
    if payload:
        failed_step = payload.get("failedStep")
        error = payload.get("error")
        if failed_step is not None or error:
            reason = f"{reason}: step={failed_step} error={error}"
    return False, msg, stderr, f"{status.lower()}: {reason}"


def summarize_result(message: dict | None) -> tuple[dict | None, dict | None]:
    if not message:
        return None, None
    result = message.get("result")
    if not isinstance(result, dict):
        return result, None
    structured = result.get("structuredContent")
    if isinstance(structured, dict):
        return result, structured
    return result, None


def main() -> int:
    parser = argparse.ArgumentParser(description="Run one app family through a persistent worker and report per-flow status.")
    parser.add_argument("--udid", required=True, help="Physical iPhone UDID to target.")
    parser.add_argument("--system", required=True, help="Workflow system namespace, e.g. reddit or linkedin.")
    parser.add_argument("--only", nargs="*", default=[], help="Optional canonical refs to run inside the selected system.")
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Directory for run artifacts.")
    parser.add_argument("--timeout-sec", type=int, default=220, help="Per-workflow timeout in seconds.")
    args = parser.parse_args()

    output_dir = Path(args.output_dir).expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    selected = {value.strip() for value in args.only if value.strip()}

    env = os.environ.copy()
    env.setdefault("RZN_IOS_RUNTIME_STATE_FILE", str(DEFAULT_STATE_FILE))
    env.setdefault("RZN_IOS_PERSIST_RUNTIME", "1")
    env.setdefault("RZN_IOS_SMART_CACHE", "1")
    env["PATH"] = f"/opt/homebrew/bin:{env.get('PATH', '')}"
    show_xcode_log = env_flag("RZN_VALIDATION_SHOW_XCODE_LOG")
    allow_provisioning_updates = env_flag("RZN_VALIDATION_ALLOW_PROVISIONING_UPDATES")
    allow_provisioning_device_registration = env_flag(
        "RZN_VALIDATION_ALLOW_PROVISIONING_DEVICE_REGISTRATION"
    )

    flows = []
    for path, workflow in catalog.parse_workflows():
        canonical = catalog.canonical_workflow_id(workflow["name"])
        system = canonical.split("/", 1)[0]
        if system != args.system:
            continue
        if selected and canonical not in selected and workflow["name"] not in selected:
            continue
        flows.append((path, workflow, canonical))

    if not flows:
        print(f"no workflows matched system={args.system!r}")
        return 1

    preflight = probe_xctrace_device_state(args.udid)
    (output_dir / "device_preflight.json").write_text(json.dumps(preflight, indent=2))
    if preflight["status"] != "available":
        reason = "device_transport"
        if preflight.get("status") == "offline":
            reason += f": xctrace_offline line={preflight.get('matchedLine')}"
        elif preflight.get("status") == "missing":
            reason += f": xctrace_missing udid={args.udid}"
        else:
            reason += f": xctrace_{preflight.get('status')}"

        results = []
        for _, workflow, canonical in flows:
            payload_args = catalog.build_args(workflow)
            row = {
                "workflow": canonical,
                "status": "BLOCKED",
                "reason": reason,
                "durationMs": 0,
                "args": payload_args,
            }
            results.append(row)
            print(f"[BLOCKED] {canonical} 0ms - {reason}", flush=True)

        summary = {
            "system": args.system,
            "udid": args.udid,
            "generatedAtEpoch": int(time.time()),
            "preflight": preflight,
            "results": results,
        }
        summary_path = output_dir / "summary.json"
        summary_path.write_text(json.dumps(summary, indent=2))
        print(json.dumps({"summaryPath": str(summary_path), "count": len(results)}, indent=2))
        return 1

    bundle_id = workflow_bundle_id(flows[0][1])
    client = WorkerClient(env)
    try:
        ok, prewarm_msg, prewarm_stderr, prewarm_reason = prewarm_session(
            client,
            args.udid,
            bundle_id,
            show_xcode_log,
            allow_provisioning_updates,
            allow_provisioning_device_registration,
        )
        if prewarm_msg:
            (output_dir / "prewarm.json").write_text(json.dumps(prewarm_msg, indent=2))
        if prewarm_stderr:
            (output_dir / "prewarm.stderr.txt").write_text(prewarm_stderr)
        prewarm_wda = None
        if "wda_build" in prewarm_reason:
            prewarm_wda = latest_wda_diagnostics()
            if prewarm_wda:
                (output_dir / "prewarm.wda.json").write_text(json.dumps(prewarm_wda, indent=2))
                if prewarm_wda.get("clue"):
                    prewarm_reason = f"{prewarm_reason} | clue={prewarm_wda['clue']}"
        if not ok:
            results = []
            for _, workflow, canonical in flows:
                payload_args = catalog.build_args(workflow)
                row = {
                    "workflow": canonical,
                    "status": "BLOCKED",
                    "reason": f"prewarm_failed: {prewarm_reason}",
                    "durationMs": 0,
                    "args": payload_args,
                }
                results.append(row)
                print(
                    f"[BLOCKED] {canonical} 0ms - prewarm_failed: {prewarm_reason}",
                    flush=True,
                )
            summary = {
                "system": args.system,
                "udid": args.udid,
                "generatedAtEpoch": int(time.time()),
                "preflight": preflight,
                "prewarmReason": prewarm_reason,
                "prewarmWda": prewarm_wda,
                "results": results,
            }
            summary_path = output_dir / "summary.json"
            summary_path.write_text(json.dumps(summary, indent=2))
            print(json.dumps({"summaryPath": str(summary_path), "count": len(results)}, indent=2))
            return 1

        results = []
        for index, (_, workflow, canonical) in enumerate(flows, 1):
            payload_args = catalog.build_args(workflow)
            if show_xcode_log:
                payload_args["showXcodeLog"] = True
            if allow_provisioning_updates:
                payload_args["allowProvisioningUpdates"] = True
            if allow_provisioning_device_registration:
                payload_args["allowProvisioningDeviceRegistration"] = True
            artifact_base = output_dir / safe_name(canonical)
            started = time.time()
            message, stderr = client.call(
                f"wf-{index}",
                "ios.workflow.run",
                {
                    "name": canonical,
                    "session": {"udid": args.udid},
                    "args": payload_args,
                    "commit": False,
                    "disconnectOnFinish": False,
                    "stopAppiumOnFinish": False,
                    "backgroundAppOnFinish": False,
                    "lockDeviceOnFinish": False,
                },
                timeout_sec=args.timeout_sec,
            )
            duration_ms = int((time.time() - started) * 1000)

            artifact_base.with_suffix(".args.json").write_text(json.dumps(payload_args, indent=2))
            artifact_base.with_suffix(".stderr.txt").write_text(stderr or "")

            result, structured = summarize_result(message)
            if message is not None:
                artifact_base.with_suffix(".rpc.json").write_text(json.dumps(message, indent=2))
            if structured is not None:
                artifact_base.with_suffix(".json").write_text(json.dumps(structured, indent=2))

            has_more_flows = index < len(flows)

            if message is None:
                status, reason = "FAIL", "worker_timeout"
                client.restart()
                if has_more_flows:
                    prewarm_session(
                        client,
                        args.udid,
                        bundle_id,
                        show_xcode_log,
                        allow_provisioning_updates,
                        allow_provisioning_device_registration,
                    )
            elif structured and structured.get("ok") is True:
                success_status = catalog.classify_success_output(structured)
                if success_status is not None:
                    status, reason = success_status
                else:
                    status, reason = "PASS", catalog.summarize_output(structured)
            else:
                payload_json = structured if isinstance(structured, dict) else result if isinstance(result, dict) else None
                stdout = json.dumps(payload_json or {}, indent=2)
                status, reason = catalog.classify_failure(stdout, stderr, payload_json)
                wda_diag = None

                if structured:
                    failed_step = structured.get("failedStep")
                    error = structured.get("error")
                    if failed_step is not None or error:
                        reason = f"{reason}: step={failed_step} error={error}"

                if "wda_build" in reason:
                    wda_diag = latest_wda_diagnostics()
                    if wda_diag:
                        artifact_base.with_suffix(".wda.json").write_text(json.dumps(wda_diag, indent=2))
                        if wda_diag.get("clue"):
                            reason = f"{reason} | clue={wda_diag['clue']}"

                client.call(
                    f"shutdown-{index}",
                    "rzn.worker.shutdown",
                    {
                        "stopAppium": False,
                        "shutdownWDA": True,
                        "backgroundApp": False,
                        "lockDevice": False,
                    },
                    timeout_sec=30,
                )
                if has_more_flows:
                    prewarm_session(
                        client,
                        args.udid,
                        bundle_id,
                        show_xcode_log,
                        allow_provisioning_updates,
                        allow_provisioning_device_registration,
                    )

            row = {
                "workflow": canonical,
                "status": status,
                "reason": reason,
                "durationMs": duration_ms,
                "args": payload_args,
            }
            results.append(row)
            print(f"[{status}] {canonical} {duration_ms}ms - {reason}", flush=True)

        summary = {
            "system": args.system,
            "udid": args.udid,
            "generatedAtEpoch": int(time.time()),
            "results": results,
        }
        summary_path = output_dir / "summary.json"
        summary_path.write_text(json.dumps(summary, indent=2))
        print(json.dumps({"summaryPath": str(summary_path), "count": len(results)}, indent=2))
        return 0
    finally:
        client.close()


if __name__ == "__main__":
    sys.exit(main())

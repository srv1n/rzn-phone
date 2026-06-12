#!/usr/bin/env python3
from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_DIR = ROOT / "crates" / "rzn_phone_worker" / "resources" / "workflows"
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


def fail(message: str) -> None:
    raise SystemExit(f"workflow metadata validation failed: {message}")


def main() -> int:
    seen: set[str] = set()
    paths = sorted(WORKFLOW_DIR.glob("*.json"))
    if not paths:
        fail(f"no workflow JSON files found in {WORKFLOW_DIR}")

    for path in paths:
        try:
            workflow = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            fail(f"{path}: invalid JSON: {exc}")

        name = workflow.get("name")
        version = workflow.get("version")
        capability = workflow.get("capability")
        steps = workflow.get("steps")
        if not isinstance(name, str) or "." not in name:
            fail(f"{path}: missing dotted workflow name")
        if name in seen:
            fail(f"{path}: duplicate workflow name {name}")
        seen.add(name)
        if not isinstance(version, str) or not SEMVER_RE.fullmatch(version):
            fail(f"{path}: invalid workflow version {version!r}")
        if workflow.get("schema_version") != "rzn.mobile.workflow.v1":
            fail(f"{path}: unsupported schema_version")
        if not isinstance(capability, dict) or not capability.get("family"):
            fail(f"{path}: missing capability family")
        if not isinstance(steps, list) or not steps:
            fail(f"{path}: workflow has no steps")

    print(f"validated {len(paths)} workflow metadata files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

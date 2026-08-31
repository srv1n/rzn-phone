#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXAMPLE_MCP_JSON = ROOT / "examples" / "rzn-phone.mcp.json"


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def validate_example_config(errors: list[str]) -> None:
    config = read_json(EXAMPLE_MCP_JSON)
    server = config.get("mcpServers", {}).get("rzn-phone", {})
    command = str(server.get("command", ""))
    env = server.get("env", {})

    if "../../dist" in command or "dist/bin" in command:
        fail(errors, f"{EXAMPLE_MCP_JSON}: command points at dist instead of the plugin root")
    if not command.endswith("/bin/macos/universal/rzn-phone-worker"):
        fail(errors, f"{EXAMPLE_MCP_JSON}: command must target the packaged worker, got {command!r}")
    if not isinstance(env, dict):
        fail(errors, f"{EXAMPLE_MCP_JSON}: env must be an object")
        return
    if not env.get("RZN_PLUGIN_DIR"):
        fail(errors, f"{EXAMPLE_MCP_JSON}: RZN_PLUGIN_DIR must be set")
    if env.get("RZN_IOS_APPIUM_URL") == "":
        fail(errors, f"{EXAMPLE_MCP_JSON}: RZN_IOS_APPIUM_URL must not be shipped as an empty string")


def kit_root(names: list[str]) -> str:
    roots = {name.split("/", 1)[0] for name in names if "/" in name}
    if len(roots) != 1:
        raise ValueError(f"expected one top-level kit directory, found {sorted(roots)}")
    return next(iter(roots))


def mode_for(info: zipfile.ZipInfo) -> int:
    return (info.external_attr >> 16) & 0o777


def validate_generated_kit(path: Path, errors: list[str]) -> None:
    if not path.exists():
        fail(errors, f"tester kit zip missing: {path}")
        return

    with zipfile.ZipFile(path) as zf:
        names = zf.namelist()
        root = kit_root(names)
        name_set = set(names)

        required = [
            "README.md",
            "scripts/tester_doctor.sh",
            "scripts/prepare_mcp_plugin.sh",
            "examples/rzn-phone.mcp.json",
        ]
        for rel in required:
            member = f"{root}/{rel}"
            if member not in name_set:
                fail(errors, f"{path}: missing {rel}")

        for rel in ["INSTALL.md", "AGENT_SETUP.md", "examples/agent-handoff.md"]:
            if f"{root}/{rel}" in name_set:
                fail(errors, f"{path}: removed narrative must not be packaged: {rel}")

        for rel in ["scripts/tester_doctor.sh", "scripts/prepare_mcp_plugin.sh"]:
            member = f"{root}/{rel}"
            if member in name_set and not (mode_for(zf.getinfo(member)) & 0o111):
                fail(errors, f"{path}: {rel} is not executable")

        artifacts = [
            name
            for name in names
            if name.startswith(f"{root}/artifacts/")
            and name.endswith("-macos_universal.zip")
            and "/rzn-phone-" in name
        ]
        if len(artifacts) != 1:
            fail(errors, f"{path}: expected exactly one plugin artifact, found {len(artifacts)}")
            return

        member = f"{root}/README.md"
        if member in name_set:
            text = zf.read(member).decode("utf-8")
            forbidden = ["install.sh", "scripts/create_tester_kit.sh"]
            for needle in forbidden:
                if needle in text:
                    fail(errors, f"{path}: README.md references non-shipped {needle}")
            if "./scripts/prepare_mcp_plugin.sh" not in text:
                fail(errors, f"{path}: README.md must document ./scripts/prepare_mcp_plugin.sh")

        with zf.open(artifacts[0]) as artifact_file:
            with zipfile.ZipFile(artifact_file) as plugin_zf:
                plugin_names = set(plugin_zf.namelist())
                if "bin/macos/universal/rzn-phone-worker" not in plugin_names:
                    fail(errors, f"{path}: plugin artifact missing worker binary")
                if "plugin.json" not in plugin_names:
                    fail(errors, f"{path}: plugin artifact missing plugin.json")
                if not any(name.startswith("resources/workflows/") for name in plugin_names):
                    fail(errors, f"{path}: plugin artifact missing workflow resources")


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate tester kit packaging and MCP config.")
    parser.add_argument("--kit", type=Path, help="Generated tester kit zip to inspect.")
    args = parser.parse_args()

    errors: list[str] = []
    validate_example_config(errors)
    if args.kit:
        validate_generated_kit(args.kit, errors)

    if errors:
        for error in errors:
            print(f"[fail] {error}", file=sys.stderr)
        return 1

    print("[ok] tester kit packaging checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

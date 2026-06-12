#!/usr/bin/env python3
from __future__ import annotations

import argparse
import difflib
import json
import os
import shlex
import sys
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def read_stdin_json() -> Any:
    return json.load(sys.stdin)


def write_json(payload: Any) -> None:
    json.dump(payload, sys.stdout, indent=2)
    sys.stdout.write("\n")


def is_tty() -> bool:
    return sys.stdout.isatty()


def canonicalize_workflow_ref(value: str) -> str:
    raw = (value or "").strip().replace("\\", "/")
    if "/" in raw:
        system, workflow = raw.split("/", 1)
    elif "." in raw:
        system, workflow = raw.split(".", 1)
    else:
        return raw.strip(" /.")
    system = system.strip(" /.")
    workflow = workflow.strip(" /.")
    return f"{system}/{workflow}" if system and workflow else raw


def command_candidates() -> list[str]:
    return [
        "devices",
        "doctor",
        "favorites",
        "favorite",
        "info",
        "list",
        "recent",
        "rerun",
        "run",
        "show",
        "shutdown",
        "status",
        "tool",
        "tools",
        "version",
        "workflow",
        "workflows",
    ]


def closest_matches(query: str, choices: list[str], limit: int = 5) -> list[str]:
    query = (query or "").strip()
    if not query:
        return []
    exact_prefix = [choice for choice in choices if choice.startswith(query)]
    ranked = difflib.get_close_matches(query, choices, n=limit, cutoff=0.35)
    merged: list[str] = []
    for candidate in exact_prefix + ranked:
        if candidate not in merged:
            merged.append(candidate)
    return merged[:limit]


def suggestion_error(kind: str, query: str, choices: list[str]) -> None:
    suggestions = closest_matches(query, choices)
    message = [f"rzn-phone: unknown {kind} '{query}'"]
    if suggestions:
        message.append("Did you mean:")
        message.extend(f"  - {candidate}" for candidate in suggestions)
    raise SystemExit("\n".join(message))


def state_dir() -> Path:
    custom = os.environ.get("RZN_PHONE_STATE_DIR")
    root = Path(custom).expanduser() if custom else Path.home() / ".rzn-phone"
    root.mkdir(parents=True, exist_ok=True)
    return root


def history_path() -> Path:
    return state_dir() / "history.jsonl"


def favorites_path() -> Path:
    return state_dir() / "favorites.json"


def load_history() -> list[dict[str, Any]]:
    path = history_path()
    if not path.exists():
        return []
    entries: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(payload, dict):
            entries.append(payload)
    return entries


def save_history(entries: list[dict[str, Any]]) -> None:
    trimmed = entries[-200:]
    text = "\n".join(json.dumps(entry, separators=(",", ":")) for entry in trimmed)
    if text:
        text += "\n"
    history_path().write_text(text, encoding="utf-8")


def load_favorites() -> list[str]:
    path = favorites_path()
    if not path.exists():
        return []
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return []
    if not isinstance(payload, list):
        return []
    out: list[str] = []
    for item in payload:
        if not isinstance(item, str):
            continue
        normalized = canonicalize_workflow_ref(item)
        if normalized and normalized not in out:
            out.append(normalized)
    return out


def save_favorites(entries: list[str]) -> None:
    deduped: list[str] = []
    for item in entries:
        normalized = canonicalize_workflow_ref(item)
        if normalized and normalized not in deduped:
            deduped.append(normalized)
    favorites_path().write_text(json.dumps(deduped, indent=2) + "\n", encoding="utf-8")


def bool_filter(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return bool(value)
    text = str(value).strip().lower()
    return text in {"1", "true", "yes", "y", "on"}


def format_kv_rows(rows: list[tuple[str, str]]) -> str:
    if not rows:
        return ""
    width = max(len(key) for key, _ in rows)
    return "\n".join(f"{key.ljust(width)}  {value}" for key, value in rows)


INPUT_GROUP_ORDER = ("core", "safety", "advanced", "internal")
INPUT_GROUP_LABELS = {
    "core": "Core Inputs",
    "safety": "Safety Gates",
    "advanced": "Advanced Inputs",
    "internal": "Internal Knobs",
}
EXAMPLE_VALUES: dict[str, Any] = {
    "query": "best headphones 2026",
    "search_query": "voice notes",
    "target_app_name": "Voicenotes AI Notes & Meetings",
    "message_text": "Quick check-in on the launch plan.",
    "comment_text": "Useful breakdown. The offline sync detail is the real win.",
    "reply_text": "That constraint makes sense. What does the fallback path look like?",
    "post_text": "Shipping the workflow help overhaul this week.",
    "updated_text": "Updated draft with clearer onboarding copy.",
    "username": "openai",
    "threadContains": "OpenAI",
    "senderContains": "OpenAI",
    "messageContains": "code",
    "country": "us",
    "locale": "en_US",
    "submit_mode": "suggestion",
    "typing_mode": "full",
    "review_sort": "most_helpful",
    "result_index": 0,
    "post_index": 0,
    "thread_index": 0,
    "reply_index": 0,
    "limit": 5,
}
ADVANCED_INPUT_EXACT = {
    "limit",
    "country",
    "locale",
    "submit_mode",
    "typing_mode",
    "review_sort",
    "capturescreenshot",
    "capturepagesource",
}
INTERNAL_INPUT_EXACT = {"maxnodes"}
ADVANCED_INPUT_PREFIXES = ("max", "min", "capture")
ADVANCED_INPUT_TOKENS = ("scroll", "timeout", "dwell", "screenshot", "page_source", "pagesource")
INTERNAL_INPUT_TOKENS = ("predicate", "selector", "xpath", "bundle_id", "bundleid", "accessibility")


def normalize_text(value: Any) -> str:
    return " ".join(str(value or "").split()).strip()


def infer_input_group(name: str, spec: dict[str, Any]) -> str:
    group = normalize_text(spec.get("group")).lower()
    if group in INPUT_GROUP_ORDER:
        return group

    lower = name.lower()
    if lower.startswith("execute_") or lower in {"submit"}:
        return "safety"
    if lower in INTERNAL_INPUT_EXACT or any(token in lower for token in INTERNAL_INPUT_TOKENS):
        return "internal"
    if spec.get("required"):
        return "core"
    if lower in ADVANCED_INPUT_EXACT:
        return "advanced"
    if lower.startswith(ADVANCED_INPUT_PREFIXES) or any(token in lower for token in ADVANCED_INPUT_TOKENS):
        return "advanced"
    return "core"


def workflow_input_groups(workflow: dict[str, Any]) -> dict[str, list[tuple[str, dict[str, Any]]]]:
    grouped: dict[str, list[tuple[str, dict[str, Any]]]] = {group: [] for group in INPUT_GROUP_ORDER}
    inputs = workflow.get("inputs") or {}
    for name in sorted(inputs):
        spec = inputs.get(name) or {}
        if not isinstance(spec, dict):
            continue
        grouped[infer_input_group(name, spec)].append((name, spec))
    return grouped


def build_input_traits(name: str, spec: dict[str, Any]) -> str:
    parts = [str(spec.get("type") or "string")]
    if spec.get("required"):
        parts.append("required")
    default = spec.get("default")
    if default is not None:
        parts.append(f"default={json.dumps(default, separators=(',', ':'))}")
    return "  ".join(parts)


def input_preview_priority(name: str) -> tuple[int, str]:
    lower = name.lower()
    if lower in {
        "query",
        "search_query",
        "message_text",
        "comment_text",
        "reply_text",
        "post_text",
        "updated_text",
        "target_app_name",
        "username",
    }:
        return (0, lower)
    if lower in {
        "limit",
        "country",
        "locale",
        "submit_mode",
        "typing_mode",
        "result_index",
        "post_index",
        "thread_index",
        "reply_index",
    }:
        return (1, lower)
    if lower.startswith("execute_") or lower == "submit":
        return (2, lower)
    if lower.startswith("capture"):
        return (4, lower)
    return (3, lower)


def inferred_input_description(name: str, spec: dict[str, Any], workflow: dict[str, Any]) -> str:
    explicit = normalize_text(spec.get("description"))
    if explicit:
        return explicit

    capability = workflow.get("capability") or {}
    mutating = bool(capability.get("mutating"))
    lower = name.lower()
    if lower in {"query", "search_query"}:
        return "Search text to submit."
    if lower in {"post_text", "comment_text", "reply_text", "message_text", "updated_text"}:
        return "Text payload to draft."
    if lower == "target_app_name":
        return "Exact app name to rank or locate."
    if lower in {"result_index", "post_index", "thread_index", "reply_index"}:
        return "Zero-based target index."
    if lower == "limit":
        return "Maximum number of items to return."
    if lower in {"country", "locale"}:
        return "Optional storefront or locale override."
    if lower.startswith("execute_"):
        action = lower.removeprefix("execute_").replace("_", " ")
        suffix = " and still requires --commit 1" if mutating else ""
        return f"Set true to actually {action}{suffix}."
    if lower == "submit":
        return "Set true to actually submit the draft."
    if infer_input_group(name, spec) == "internal":
        return "Low-level workflow tuning. Leave this alone unless you are debugging."
    return ""


def inferred_input_example(name: str, spec: dict[str, Any]) -> Any:
    if "example" in spec and spec.get("example") is not None:
        return spec.get("example")
    if name in EXAMPLE_VALUES:
        return EXAMPLE_VALUES[name]

    kind = str(spec.get("type") or "string")
    if kind == "string":
        return f"<{name}>"
    if kind in {"integer", "number"}:
        return 1
    if kind == "boolean":
        return False
    if kind == "array":
        return [f"<{name}_item>"]
    if kind == "object":
        return {}
    return f"<{name}>"


def build_example_args(
    workflow: dict[str, Any],
    *,
    include_advanced: bool = False,
    include_safety: bool = False,
) -> dict[str, Any]:
    example: dict[str, Any] = {}
    grouped = workflow_input_groups(workflow)
    wanted_groups = {"core"}
    if include_safety:
        wanted_groups.add("safety")
    if include_advanced:
        wanted_groups.add("advanced")

    for group in INPUT_GROUP_ORDER:
        if group not in wanted_groups:
            continue
        for key, spec in grouped[group]:
            default = spec.get("default")
            if group != "safety" and default is not None and not spec.get("required"):
                continue
            example[key] = inferred_input_example(key, spec)
    return example


def example_command(workflow: dict[str, Any], args_payload: dict[str, Any]) -> str:
    capability = workflow.get("capability") or {}
    example_json = json.dumps(args_payload, separators=(",", ":"))
    command = f"rzn-phone run {workflow.get('id')} --udid <udid> --args-json '{example_json}'"
    if capability.get("mutating") and not any(key.startswith("execute_") or key == "submit" for key in args_payload):
        command += " --dry-run"
    elif capability.get("mutating") and any(bool(value) for key, value in args_payload.items() if key.startswith("execute_") or key == "submit"):
        command += " --commit 1"
    return command


def workflow_examples(workflow: dict[str, Any], expanded: bool) -> list[dict[str, Any]]:
    help_block = workflow.get("help") or {}
    examples = help_block.get("examples") or []
    out: list[dict[str, Any]] = []
    for item in examples:
        if not isinstance(item, dict):
            continue
        args_payload = item.get("args") or {}
        if not isinstance(args_payload, dict):
            continue
        out.append(
            {
                "label": normalize_text(item.get("label")) or "Example",
                "description": normalize_text(item.get("description")),
                "args": args_payload,
            }
        )

    if out and expanded:
        return out
    if out:
        return out[:1]

    fallback = [
        {
            "label": "Quick Start",
            "description": "",
            "args": build_example_args(workflow),
        }
    ]
    capability = workflow.get("capability") or {}
    if expanded and capability.get("mutating"):
        live_args = build_example_args(workflow, include_safety=True)
        for key in list(live_args):
            if key.startswith("execute_") or key == "submit":
                live_args[key] = True
        fallback.append(
            {
                "label": "Live Run",
                "description": "Same workflow, but with the safety gate enabled.",
                "args": live_args,
            }
        )
    return fallback


def workflow_contract_preview(workflow: dict[str, Any]) -> str:
    grouped = workflow_input_groups(workflow)
    parts: list[str] = []
    core = sorted(
        [name for name, spec in grouped["core"] if spec.get("required")],
        key=input_preview_priority,
    )
    if core:
        parts.append(f"needs {', '.join(core[:3])}")
        if len(core) > 3:
            parts[-1] += f" +{len(core) - 3}"
    elif any(grouped.values()):
        optional_core = sorted(
            [name for name, _ in grouped["core"]],
            key=input_preview_priority,
        )[:2]
        if optional_core:
            parts.append(f"start with {', '.join(optional_core)}")

    advanced = sorted(
        [name for name, _ in grouped["advanced"]],
        key=input_preview_priority,
    )[:2]
    if advanced:
        parts.append(f"opt {', '.join(advanced)}")

    safety = sorted(
        [name for name, _ in grouped["safety"]],
        key=input_preview_priority,
    )
    if safety:
        gate_text = ", ".join(safety[:2])
        if len(safety) > 2:
            gate_text += f" +{len(safety) - 2}"
        parts.append(f"gate {gate_text} + --commit 1")

    return " | ".join(parts) if parts else "no workflow args"


def summarize_workflow(workflow: dict[str, Any], favorites: set[str], *, detailed: bool) -> list[str]:
    cap = workflow.get("capability") or {}
    family = cap.get("family") or "other"
    surface = cap.get("surface") or "unknown"
    mutating = cap.get("mutating")
    mode = "write" if mutating else "read"
    star = "*" if workflow.get("id") in favorites else " "
    header = (
        f"{star} {workflow.get('id','?'):<28} "
        f"{family:<10} {surface:<11} {mode:<5} "
        f"{normalize_text(workflow.get('description'))}"
    ).rstrip()
    if not detailed:
        return [header]
    return [header, f"    {workflow_contract_preview(workflow)}"]


def resolve_system_filter(
    payload: dict[str, Any], raw_system: str | None, search: str | None
) -> tuple[str | None, str | None, str | None]:
    system = normalize_text(raw_system)
    search_text = normalize_text(search) or None
    if not system:
        return None, search_text, None

    systems = {
        str(item.get("id") or ""): str(item.get("id") or "")
        for item in payload.get("systems", [])
        if isinstance(item, dict)
    }
    for system_id in systems.values():
        if system_id.lower() == system.lower():
            return system_id, search_text, None
    if search_text:
        return None, f"{system} {search_text}", f"Positional query fallback: {system}"
    return None, system, f"Positional query fallback: {system}"


def workflow_matches_filters(
    workflow: dict[str, Any],
    *,
    system: str | None,
    search: str | None,
    mutating: bool | None,
    surface: str | None,
    has_input: str | None,
    favorites_only: bool,
    favorites: set[str],
) -> bool:
    capability = workflow.get("capability") or {}
    if favorites_only and workflow.get("id") not in favorites:
        return False
    if system and str(workflow.get("system") or "") != system:
        return False
    if mutating is not None and bool(capability.get("mutating")) != mutating:
        return False
    if surface and str(capability.get("surface") or "").lower() != surface.lower():
        return False
    if has_input:
        inputs = workflow.get("inputs") or {}
        if has_input not in inputs:
            return False
    if search:
        help_block = workflow.get("help") or {}
        haystack = "\n".join(
            str(value)
            for value in [
                workflow.get("id"),
                workflow.get("name"),
                workflow.get("system"),
                workflow.get("workflow"),
                workflow.get("description"),
                capability.get("family"),
                capability.get("intent"),
                capability.get("surface"),
                " ".join((workflow.get("inputs") or {}).keys()),
                " ".join(
                    normalize_text((spec or {}).get("description"))
                    for spec in (workflow.get("inputs") or {}).values()
                    if isinstance(spec, dict)
                ),
                help_block.get("when_to_use"),
                help_block.get("returns"),
            ]
            if value
        ).lower()
        if search.lower() not in haystack:
            return False
    return True


def filtered_workflow_payload(
    payload: dict[str, Any],
    *,
    system: str | None,
    search: str | None,
    mutating: bool | None,
    surface: str | None,
    has_input: str | None,
    favorites_only: bool,
) -> dict[str, Any]:
    favorites = set(load_favorites())
    workflows = [
        workflow
        for workflow in payload.get("workflows", [])
        if isinstance(workflow, dict)
        and workflow_matches_filters(
            workflow,
            system=system,
            search=search,
            mutating=mutating,
            surface=surface,
            has_input=has_input,
            favorites_only=favorites_only,
            favorites=favorites,
        )
    ]
    systems_map: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for workflow in workflows:
        systems_map[str(workflow.get("system") or "misc")].append(workflow)
    systems = []
    for system_id in sorted(systems_map):
        group = sorted(systems_map[system_id], key=lambda item: str(item.get("id") or ""))
        systems.append(
            {
                "id": system_id,
                "workflow_count": len(group),
                "workflows": group,
            }
        )
    return {
        "systemCount": len(systems),
        "workflowCount": len(workflows),
        "systems": systems,
        "workflows": sorted(workflows, key=lambda item: str(item.get("id") or "")),
    }


def workflow_list_cmd(args: argparse.Namespace) -> None:
    source_payload = read_stdin_json()
    system_filter, search_filter, system_note = resolve_system_filter(
        source_payload, args.system, args.search
    )
    payload = filtered_workflow_payload(
        source_payload,
        system=system_filter,
        search=search_filter,
        mutating=args.mutating,
        surface=args.surface,
        has_input=args.has_input,
        favorites_only=args.favorites,
    )
    if args.json or (not args.pretty and not is_tty()):
        write_json(payload)
        return

    favorites = set(load_favorites())
    lines = [
        f"Systems: {payload['systemCount']}  Workflows: {payload['workflowCount']}",
    ]
    if system_filter:
        lines.append(f"System: {system_filter}")
    if search_filter:
        lines.append(f"Search: {search_filter}")
    if system_note:
        lines.append(system_note)
    if args.favorites:
        lines.append("Filter: favorites")
    if payload["workflowCount"] == 0:
        lines.append("No workflows found.")
        sys.stdout.write("\n".join(lines) + "\n")
        return

    detailed = bool(
        search_filter
        or system_filter
        or args.has_input
        or args.surface
        or args.favorites
        or payload["workflowCount"] <= 12
    )
    for system in payload["systems"]:
        lines.append("")
        lines.append(f"{system['id']} ({system['workflow_count']})")
        if args.compact:
            names = ", ".join(workflow.get("workflow", "?") for workflow in system["workflows"])
            lines.append(f"  {names}")
            continue
        for workflow in system["workflows"]:
            lines.extend(f"  {line}" if idx == 0 else line for idx, line in enumerate(summarize_workflow(workflow, favorites, detailed=detailed)))
    if detailed and payload["workflowCount"]:
        lines.extend(
            [
                "",
                "Next",
                "  Use `rzn-phone show <workflow>` for full input docs and runnable examples.",
            ]
        )
    if favorites:
        lines.append("")
        lines.append("* favorite")
    sys.stdout.write("\n".join(lines) + "\n")


def workflow_show_cmd(args: argparse.Namespace) -> None:
    workflow = read_stdin_json()
    if args.json or (not args.pretty and not is_tty() and not args.example):
        write_json(workflow)
        return

    capability = workflow.get("capability") or {}
    inputs = workflow.get("inputs") or {}
    help_block = workflow.get("help") or {}
    notes = [normalize_text(item) for item in workflow.get("notes") or [] if normalize_text(item)]
    rows = [
        ("ID", str(workflow.get("id") or "")),
        ("Version", str(workflow.get("version") or "")),
        ("Family", str(capability.get("family") or "other")),
        ("Intent", str(capability.get("intent") or "n/a")),
        ("Surface", str(capability.get("surface") or "n/a")),
        ("Mode", "write" if capability.get("mutating") else "read"),
    ]
    lines = [format_kv_rows(rows), "", normalize_text(workflow.get("description"))]

    when_to_use = normalize_text(help_block.get("when_to_use"))
    if when_to_use:
        lines.extend(["", "Use It When", f"  {when_to_use}"])

    examples = workflow_examples(workflow, expanded=args.example)
    if examples:
        lines.extend(["", "Quick Start"])
        quick = examples[0]
        if quick["description"]:
            lines.append(f"  {quick['description']}")
        lines.append(f"  {example_command(workflow, quick['args'])}")

    grouped_inputs = workflow_input_groups(workflow)
    if inputs:
        for group in INPUT_GROUP_ORDER:
            items = grouped_inputs[group]
            if not items:
                continue
            width = max(len(name) for name, _ in items)
            lines.extend(["", INPUT_GROUP_LABELS[group]])
            for name, spec in items:
                traits = build_input_traits(name, spec)
                description = inferred_input_description(name, spec, workflow)
                example_value = inferred_input_example(name, spec)
                line = f"  {name.ljust(width)}  {traits}"
                if description:
                    line += f"  {description}"
                if example_value is not None:
                    line += f"  e.g. {json.dumps(example_value, separators=(',', ':'))}"
                lines.append(line)
    else:
        lines.extend(["", "Inputs", "  none"])

    returns = normalize_text(help_block.get("returns"))
    if returns:
        lines.extend(["", "Returns", f"  {returns}"])

    if notes:
        lines.extend(["", "Notes"])
        lines.extend(f"  - {note}" for note in notes)

    if args.example and len(examples) > 1:
        lines.extend(["", "Examples"])
        for example in examples[1:]:
            lines.append(f"  {example['label']}")
            if example["description"]:
                lines.append(f"    {example['description']}")
            lines.append(f"    {example_command(workflow, example['args'])}")

    sys.stdout.write("\n".join(lines).rstrip() + "\n")


def tool_matches_filters(
    tool: dict[str, Any],
    *,
    search: str | None,
    family: str | None,
    tier: str | None,
) -> bool:
    if family and str(tool.get("capabilityFamily") or "").lower() != family.lower():
        return False
    if tier and str(tool.get("capabilityTier") or "").lower() != tier.lower():
        return False
    if search:
        haystack = "\n".join(
            str(value)
            for value in [
                tool.get("name"),
                tool.get("description"),
                tool.get("capabilityFamily"),
                tool.get("capabilityTier"),
                " ".join(((tool.get("inputSchema") or {}).get("properties") or {}).keys()),
            ]
            if value
        ).lower()
        if search.lower() not in haystack:
            return False
    return True


def tool_list_cmd(args: argparse.Namespace) -> None:
    payload = read_stdin_json()
    tools = [
        tool
        for tool in payload.get("tools", [])
        if isinstance(tool, dict)
        and tool_matches_filters(
            tool,
            search=args.search,
            family=args.family,
            tier=args.tier,
        )
    ]
    tools.sort(key=lambda item: str(item.get("name") or ""))
    filtered = {"tools": tools}
    if args.json or (not args.pretty and not is_tty()):
        write_json(filtered)
        return

    lines = [f"Tools: {len(tools)}"]
    if not tools:
        lines.append("No tools found.")
        sys.stdout.write("\n".join(lines) + "\n")
        return

    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for tool in tools:
        grouped[str(tool.get("capabilityFamily") or "other")].append(tool)

    for family in sorted(grouped):
        lines.append("")
        lines.append(f"{family} ({len(grouped[family])})")
        for tool in grouped[family]:
            lines.append(
                f"  {str(tool.get('name') or '').ljust(34)} "
                f"tier {tool.get('capabilityTier', '?')}  {tool.get('description', '').strip()}"
            )
    sys.stdout.write("\n".join(lines) + "\n")


def tool_show_cmd(args: argparse.Namespace) -> None:
    tool = read_stdin_json()
    if args.json or (not args.pretty and not is_tty()):
        write_json(tool)
        return

    rows = [
        ("Name", str(tool.get("name") or "")),
        ("Family", str(tool.get("capabilityFamily") or "other")),
        ("Tier", str(tool.get("capabilityTier") or "?")),
    ]
    lines = [format_kv_rows(rows), "", str(tool.get("description") or "").strip()]
    schema = tool.get("inputSchema") or {}
    props = schema.get("properties") or {}
    required = set(schema.get("required") or [])
    lines.extend(["", "Inputs"])
    if not props:
        lines.append("  none")
    else:
        width = max(len(name) for name in props)
        for name in sorted(props):
            spec = props.get(name) or {}
            parts = [str(spec.get("type") or "any")]
            if name in required:
                parts.append("required")
            if spec.get("description"):
                parts.append(str(spec["description"]))
            lines.append(f"  {name.ljust(width)}  {'  '.join(parts)}")
    sys.stdout.write("\n".join(lines).rstrip() + "\n")


def capability_list_cmd(args: argparse.Namespace) -> None:
    payload = read_stdin_json()
    if args.json or (not args.pretty and not is_tty()):
        write_json(payload)
        return

    tool_counts = {
        item.get("family"): len(item.get("tools") or [])
        for item in payload.get("toolFamilies", [])
        if isinstance(item, dict)
    }
    workflow_counts = {
        item.get("family"): len(item.get("workflows") or [])
        for item in payload.get("workflowFamilies", [])
        if isinstance(item, dict)
    }
    lines = ["Capability Families"]
    for family in payload.get("families", []):
        family_id = str(family.get("id") or "")
        lines.append("")
        lines.append(
            f"{family_id}  tier {family.get('tier')}  "
            f"tools {tool_counts.get(family_id, 0)}  "
            f"workflows {workflow_counts.get(family_id, 0)}"
        )
        lines.append(f"  {str(family.get('description') or '').strip()}")
        examples = family.get("examples") or []
        if examples:
            lines.append(f"  examples: {', '.join(str(item) for item in examples)}")
    sys.stdout.write("\n".join(lines) + "\n")


def devices_cmd(args: argparse.Namespace) -> None:
    payload = read_stdin_json()
    if args.json or (not args.pretty and not is_tty()):
        write_json(payload)
        return

    devices = payload.get("devices") or []
    lines = [f"Devices: {len(devices)}"]
    if not devices:
        lines.append("No physical devices found.")
        sys.stdout.write("\n".join(lines) + "\n")
        return
    for device in devices:
        status = "available" if device.get("is_available") else "offline"
        lines.append(
            f"  {str(device.get('name') or '').ljust(24)} "
            f"iOS {str(device.get('platform_version') or '').ljust(6)} "
            f"{status.ljust(9)} {device.get('udid')}"
        )
    sys.stdout.write("\n".join(lines) + "\n")


def workflow_select_cmd(args: argparse.Namespace) -> None:
    want = canonicalize_workflow_ref(args.ref)
    payload = read_stdin_json()
    workflows = [item for item in payload.get("workflows", []) if isinstance(item, dict)]
    for workflow in workflows:
        candidates = {
            str(workflow.get("id") or ""),
            str(workflow.get("name") or ""),
            canonicalize_workflow_ref(str(workflow.get("id") or "")),
            canonicalize_workflow_ref(str(workflow.get("name") or "")),
        }
        if want in candidates:
            write_json(workflow)
            return
    suggestion_error("workflow", want, [str(item.get("id") or "") for item in workflows])


def tool_select_cmd(args: argparse.Namespace) -> None:
    want = args.name
    payload = read_stdin_json()
    tools = [item for item in payload.get("tools", []) if isinstance(item, dict)]
    for tool in tools:
        if tool.get("name") == want:
            write_json(tool)
            return
    suggestion_error("tool", want, [str(item.get("name") or "") for item in tools])


def select_default_device_cmd(_: argparse.Namespace) -> None:
    payload = read_stdin_json()
    devices = [
        device
        for device in payload.get("devices", [])
        if isinstance(device, dict)
        and not device.get("is_simulator")
        and device.get("is_available")
    ]
    if len(devices) == 1:
        sys.stdout.write(str(devices[0].get("udid") or "") + "\n")
        return
    if not devices:
        raise SystemExit("rzn-phone: no available physical devices found; run `rzn-phone devices`")
    names = ", ".join(str(device.get("name") or device.get("udid") or "?") for device in devices)
    raise SystemExit(
        "rzn-phone: multiple available devices found; pass --udid explicitly "
        f"({names})"
    )


def history_append_cmd(args: argparse.Namespace) -> None:
    try:
        args_payload = json.loads(args.args_json)
    except json.JSONDecodeError:
        args_payload = {"_raw": args.args_json}
    entry = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "workflowRef": canonicalize_workflow_ref(args.workflow_ref),
        "udid": args.udid,
        "argsJson": args_payload,
        "commit": bool_filter(args.commit),
        "disconnectOnFinish": bool_filter(args.disconnect_on_finish),
        "stopAppiumOnFinish": bool_filter(args.stop_appium_on_finish),
        "backgroundOnExit": bool_filter(args.background_on_exit),
        "lockDeviceOnExit": bool_filter(args.lock_device_on_exit),
        "smartCache": bool_filter(args.smart_cache),
    }
    entries = load_history()
    entries.append(entry)
    save_history(entries)


def recent_cmd(args: argparse.Namespace) -> None:
    entries = list(reversed(load_history()))[: args.limit]
    if args.json or (not args.pretty and not is_tty()):
        write_json(entries)
        return

    favorites = set(load_favorites())
    lines = [f"Recent Runs: {len(entries)}"]
    if not entries:
        lines.append("No recent workflow runs.")
        sys.stdout.write("\n".join(lines) + "\n")
        return
    for idx, entry in enumerate(entries, start=1):
        ts = str(entry.get("ts") or "")
        try:
            when = datetime.fromisoformat(ts.replace("Z", "+00:00")).astimezone().strftime(
                "%Y-%m-%d %H:%M"
            )
        except ValueError:
            when = ts
        ref = str(entry.get("workflowRef") or "")
        star = "*" if ref in favorites else " "
        mode = "live" if entry.get("commit") else "dry-run"
        udid = str(entry.get("udid") or "")
        lines.append(
            f"{idx:>2}. {star} {ref}  [{mode}]  {when}  {udid[:8]}"
        )
        args_json = entry.get("argsJson")
        if args_json not in ({}, None):
            lines.append(f"    args: {json.dumps(args_json, separators=(',', ':'))}")
    if favorites:
        lines.append("")
        lines.append("* favorite")
    sys.stdout.write("\n".join(lines) + "\n")


def rerun_show_cmd(args: argparse.Namespace) -> None:
    entries = list(reversed(load_history()))
    if args.index < 1 or args.index > len(entries):
        raise SystemExit(
            f"rzn-phone: recent entry {args.index} does not exist; run `rzn-phone recent`"
        )
    write_json(entries[args.index - 1])


def favorite_add_cmd(args: argparse.Namespace) -> None:
    ref = canonicalize_workflow_ref(args.ref)
    favorites = load_favorites()
    if ref not in favorites:
        favorites.append(ref)
        save_favorites(favorites)
    sys.stdout.write(f"Favorited {ref}\n")


def favorite_remove_cmd(args: argparse.Namespace) -> None:
    ref = canonicalize_workflow_ref(args.ref)
    favorites = [item for item in load_favorites() if item != ref]
    save_favorites(favorites)
    sys.stdout.write(f"Removed {ref}\n")


def favorite_list_cmd(args: argparse.Namespace) -> None:
    favorites = load_favorites()
    if args.json or (not args.pretty and not is_tty()):
        write_json(favorites)
        return
    lines = [f"Favorites: {len(favorites)}"]
    if favorites:
        lines.extend(f"  {item}" for item in favorites)
    else:
        lines.append("No favorite workflows.")
    sys.stdout.write("\n".join(lines) + "\n")


def complete_cmd(args: argparse.Namespace) -> None:
    payload = read_stdin_json() if not sys.stdin.isatty() else None
    values: list[str] = []
    if args.entity == "commands":
        values = command_candidates()
    elif args.entity == "workflows" and isinstance(payload, dict):
        values = [str(item.get("id") or "") for item in payload.get("workflows", []) if isinstance(item, dict)]
    elif args.entity == "systems" and isinstance(payload, dict):
        values = [str(item.get("id") or "") for item in payload.get("systems", []) if isinstance(item, dict)]
    elif args.entity == "tools" and isinstance(payload, dict):
        values = [str(item.get("name") or "") for item in payload.get("tools", []) if isinstance(item, dict)]
    elif args.entity == "families" and isinstance(payload, dict):
        values = [str(item.get("id") or "") for item in payload.get("families", []) if isinstance(item, dict)]
    elif args.entity == "favorites":
        values = load_favorites()
    for value in sorted(item for item in values if item):
        print(value)


def completion_script_cmd(args: argparse.Namespace) -> None:
    command_name = args.command_name
    if args.shell == "bash":
        sys.stdout.write(
            f"""# bash completion for {command_name}
_{command_name.replace('-', '_')}_complete() {{
  local cur prev words cword
  _init_completion || return
  case "$prev" in
    --udid)
      COMPREPLY=( $(compgen -W "$({command_name} devices --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\" \".join(d.get(\"udid\",\"\") for d in p.get(\"devices\", [])))' 2>/dev/null)" -- "$cur") )
      return
      ;;
    --family)
      COMPREPLY=( $(compgen -W "$({command_name} capability list --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\" \".join(f.get(\"id\",\"\") for f in p.get(\"families\", [])))' 2>/dev/null)" -- "$cur") )
      return
      ;;
  esac
  if [[ $cword -eq 1 ]]; then
    COMPREPLY=( $(compgen -W "$({command_name} __complete commands)" -- "$cur") )
    return
  fi
  case "${{words[1]}}" in
    run|show)
      COMPREPLY=( $(compgen -W "$({command_name} list --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\" \".join(w.get(\"id\",\"\") for w in p.get(\"workflows\", [])))' 2>/dev/null)" -- "$cur") )
      ;;
    workflow)
      if [[ "${{words[2]}}" == "show" ]]; then
        COMPREPLY=( $(compgen -W "$({command_name} list --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\" \".join(w.get(\"id\",\"\") for w in p.get(\"workflows\", [])))' 2>/dev/null)" -- "$cur") )
      fi
      ;;
    tool|tools)
      COMPREPLY=( $(compgen -W "$({command_name} tool list --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\" \".join(t.get(\"name\",\"\") for t in p.get(\"tools\", [])))' 2>/dev/null)" -- "$cur") )
      ;;
    favorite|favorites)
      if [[ "${{words[2]}}" == "add" || "${{words[2]}}" == "remove" ]]; then
        COMPREPLY=( $(compgen -W "$({command_name} list --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\" \".join(w.get(\"id\",\"\") for w in p.get(\"workflows\", [])))' 2>/dev/null)" -- "$cur") )
      fi
      ;;
  esac
}}
complete -F _{command_name.replace('-', '_')}_complete {command_name}
"""
        )
        return

    if args.shell == "zsh":
        sys.stdout.write(
            f"""#compdef {command_name}

_{command_name.replace('-', '_')}_commands() {{
  local -a cmds
  cmds=(
    'doctor:Check local prerequisites'
    'devices:List connected physical iPhones'
    'favorite:Manage favorite workflows'
    'favorites:List favorite workflows'
    'info:Show install metadata'
    'list:List workflows grouped by system'
    'recent:Show recent workflow runs'
    'rerun:Rerun a previous workflow'
    'run:Run a workflow'
    'show:Show a workflow or tool'
    'shutdown:Shutdown active runtime'
    'status:Show runtime status'
    'tool:Inspect or call a tool'
    'tools:Alias for tool list'
    'version:Show version'
    'workflow:Inspect workflows'
    'workflows:Manage workflow packs'
  )
  _describe 'command' cmds
}}

_{command_name.replace('-', '_')}() {{
  if (( CURRENT == 2 )); then
    _{command_name.replace('-', '_')}_commands
    return
  fi
  case "$words[2]" in
    run|show)
      local -a refs
      refs=("${{(@f)$({command_name} list --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\"\\n\".join(w.get(\"id\",\"\") for w in p.get(\"workflows\", [])))' 2>/dev/null)}}")
      _describe 'workflow' refs
      ;;
    tool|tools)
      local -a tools
      tools=("${{(@f)$({command_name} tool list --json 2>/dev/null | python3 -c 'import json,sys; p=json.load(sys.stdin); print(\"\\n\".join(t.get(\"name\",\"\") for t in p.get(\"tools\", [])))' 2>/dev/null)}}")
      _describe 'tool' tools
      ;;
    *)
      _arguments '*::arg: '
      ;;
  esac
}}

_{command_name.replace('-', '_')} "$@"
"""
        )
        return
    raise SystemExit(f"unsupported shell: {args.shell}")


def suggest_command_cmd(args: argparse.Namespace) -> None:
    suggestions = closest_matches(args.query, command_candidates())
    if suggestions:
        sys.stdout.write("\n".join(suggestions) + "\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(add_help=False)
    sub = parser.add_subparsers(dest="cmd", required=True)

    workflow_list = sub.add_parser("workflow-list")
    workflow_list.add_argument("--json", action="store_true")
    workflow_list.add_argument("--pretty", action="store_true")
    workflow_list.add_argument("--compact", action="store_true")
    workflow_list.add_argument("--system")
    workflow_list.add_argument("--search")
    workflow_list.add_argument("--surface")
    workflow_list.add_argument("--has-input")
    workflow_list.add_argument("--favorites", action="store_true")
    workflow_list.add_argument(
        "--mutating",
        type=lambda value: bool_filter(value),
        nargs="?",
        const="1",
    )
    workflow_list.set_defaults(func=workflow_list_cmd)

    workflow_show = sub.add_parser("workflow-show")
    workflow_show.add_argument("--json", action="store_true")
    workflow_show.add_argument("--pretty", action="store_true")
    workflow_show.add_argument("--example", action="store_true")
    workflow_show.set_defaults(func=workflow_show_cmd)

    tool_list = sub.add_parser("tool-list")
    tool_list.add_argument("--json", action="store_true")
    tool_list.add_argument("--pretty", action="store_true")
    tool_list.add_argument("--search")
    tool_list.add_argument("--family")
    tool_list.add_argument("--tier")
    tool_list.set_defaults(func=tool_list_cmd)

    tool_show = sub.add_parser("tool-show")
    tool_show.add_argument("--json", action="store_true")
    tool_show.add_argument("--pretty", action="store_true")
    tool_show.set_defaults(func=tool_show_cmd)

    capability_list = sub.add_parser("capability-list")
    capability_list.add_argument("--json", action="store_true")
    capability_list.add_argument("--pretty", action="store_true")
    capability_list.set_defaults(func=capability_list_cmd)

    devices = sub.add_parser("devices")
    devices.add_argument("--json", action="store_true")
    devices.add_argument("--pretty", action="store_true")
    devices.set_defaults(func=devices_cmd)

    wf_select = sub.add_parser("workflow-select")
    wf_select.add_argument("ref")
    wf_select.set_defaults(func=workflow_select_cmd)

    tool_select = sub.add_parser("tool-select")
    tool_select.add_argument("name")
    tool_select.set_defaults(func=tool_select_cmd)

    select_default_device = sub.add_parser("select-default-device")
    select_default_device.set_defaults(func=select_default_device_cmd)

    history_append = sub.add_parser("history-append")
    history_append.add_argument("--workflow-ref", required=True)
    history_append.add_argument("--udid", required=True)
    history_append.add_argument("--args-json", required=True)
    history_append.add_argument("--commit", required=True)
    history_append.add_argument("--disconnect-on-finish", required=True)
    history_append.add_argument("--stop-appium-on-finish", required=True)
    history_append.add_argument("--background-on-exit", required=True)
    history_append.add_argument("--lock-device-on-exit", required=True)
    history_append.add_argument("--smart-cache", required=True)
    history_append.set_defaults(func=history_append_cmd)

    recent = sub.add_parser("recent")
    recent.add_argument("--json", action="store_true")
    recent.add_argument("--pretty", action="store_true")
    recent.add_argument("--limit", type=int, default=10)
    recent.set_defaults(func=recent_cmd)

    rerun_show = sub.add_parser("rerun-show")
    rerun_show.add_argument("index", type=int)
    rerun_show.set_defaults(func=rerun_show_cmd)

    favorite_add = sub.add_parser("favorite-add")
    favorite_add.add_argument("ref")
    favorite_add.set_defaults(func=favorite_add_cmd)

    favorite_remove = sub.add_parser("favorite-remove")
    favorite_remove.add_argument("ref")
    favorite_remove.set_defaults(func=favorite_remove_cmd)

    favorite_list = sub.add_parser("favorite-list")
    favorite_list.add_argument("--json", action="store_true")
    favorite_list.add_argument("--pretty", action="store_true")
    favorite_list.set_defaults(func=favorite_list_cmd)

    complete = sub.add_parser("complete")
    complete.add_argument("entity")
    complete.set_defaults(func=complete_cmd)

    completion_script = sub.add_parser("completion-script")
    completion_script.add_argument("shell", choices=["bash", "zsh"])
    completion_script.add_argument("--command-name", default="rzn-phone")
    completion_script.set_defaults(func=completion_script_cmd)

    suggest_command = sub.add_parser("suggest-command")
    suggest_command.add_argument("query")
    suggest_command.set_defaults(func=suggest_command_cmd)

    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()

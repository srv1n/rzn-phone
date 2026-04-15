#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path


DEFAULT_MODEL = "gpt-5.4-mini"
ROOT = Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate GitHub release notes from git history, optionally using OpenAI."
    )
    parser.add_argument("--current-tag", required=True, help="Current release tag, e.g. v0.2.0")
    parser.add_argument("--output", required=True, help="Output markdown file")
    parser.add_argument(
        "--repo",
        default="",
        help="Repository in owner/name form for compare links. Auto-detected when omitted.",
    )
    return parser.parse_args()


def run_git(args: list[str], *, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=check,
    )


def git_output(args: list[str], *, check: bool = True) -> str:
    return run_git(args, check=check).stdout.strip()


def previous_tag(current_tag: str) -> str:
    result = run_git(["describe", "--tags", "--abbrev=0", f"{current_tag}^"], check=False)
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


def detect_repo() -> str:
    remote = git_output(["remote", "get-url", "origin"])
    match = re.search(r"github\.com[:/](?P<repo>[^/]+/[^/.]+)(?:\.git)?$", remote)
    return match.group("repo") if match else ""


def commit_lines(start_tag: str, end_tag: str) -> list[str]:
    ref = f"{start_tag}..{end_tag}" if start_tag else end_tag
    raw = git_output(["log", "--no-merges", "--format=%h %s", ref])
    return [line for line in raw.splitlines() if line.strip()]


def changed_files(start_tag: str, end_tag: str) -> list[str]:
    ref = f"{start_tag}..{end_tag}" if start_tag else end_tag
    raw = git_output(["diff", "--name-only", ref])
    return [line for line in raw.splitlines() if line.strip()]


def shortstat(start_tag: str, end_tag: str) -> str:
    ref = f"{start_tag}..{end_tag}" if start_tag else end_tag
    return git_output(["diff", "--shortstat", ref]) or "No file-level diff summary available."


def compare_url(repo: str, start_tag: str, end_tag: str) -> str:
    if not repo or not start_tag:
        return ""
    return f"https://github.com/{repo}/compare/{start_tag}...{end_tag}"


def fallback_notes(
    *,
    repo: str,
    start_tag: str,
    end_tag: str,
    commit_items: list[str],
    files: list[str],
    diff_summary: str,
) -> str:
    highlights = commit_items[:8] or ["Initial public release cut."]
    file_lines = files[:10]
    lines = [
        "## Highlights",
        "",
        f"- Release `{end_tag}` packages the latest `rzn-phone` worker/runtime changes.",
        f"- Diff summary: {diff_summary}",
        "- Full local iOS automation still targets macOS + Xcode; Linux and Windows assets are standalone worker bundles for controller and integration scenarios.",
        "",
        "## What Changed",
        "",
    ]
    lines.extend(f"- {item}" for item in highlights)
    if file_lines:
        lines.extend(["", "## Touchpoints", ""])
        lines.extend(f"- `{item}`" for item in file_lines)
    lines.extend(
        [
            "",
            "## Install",
            "",
            "- macOS: use `rzn-phone-<version>-macos_universal.tar.gz` plus `rzn-phone-install.sh`.",
            "- Linux/Windows/macOS arch-specific assets ship the standalone `rzn-phone-worker` bundle.",
        ]
    )
    url = compare_url(repo, start_tag, end_tag)
    if url:
        lines.extend(["", f"[Full Changelog]({url})"])
    return "\n".join(lines) + "\n"


def llm_prompt(
    *,
    repo: str,
    start_tag: str,
    end_tag: str,
    commit_items: list[str],
    files: list[str],
    diff_summary: str,
) -> str:
    compare = compare_url(repo, start_tag, end_tag)
    commit_block = "\n".join(f"- {item}" for item in commit_items[:80]) or "- Initial release"
    file_block = "\n".join(f"- {item}" for item in files[:120]) or "- No changed files captured"
    return f"""Write release notes for `{end_tag}` based only on the git data below.

Requirements:
- Return Markdown only.
- Keep it tight and useful, under 350 words.
- Use these headings exactly: `## Highlights`, `## What Changed`, `## Install`.
- Do not add contributor/author acknowledgements.
- Do not pretend Linux or Windows can run local Xcode/iOS automation. Full local iOS execution still requires macOS + Xcode.
- Mention the release assets plainly: macOS universal runtime, macOS Intel worker, macOS Apple Silicon worker, Linux x86_64 worker, Windows x86_64 worker, and the workflow pack.
- If a compare URL is provided, end with a `Full Changelog` link.

Current tag: {end_tag}
Previous tag: {start_tag or "none"}
Diff summary: {diff_summary}
Compare URL: {compare or "none"}

Commits:
{commit_block}

Changed files:
{file_block}
"""


def extract_response_text(payload: dict) -> str:
    direct = payload.get("output_text")
    if isinstance(direct, str) and direct.strip():
        return direct.strip()

    chunks: list[str] = []
    for item in payload.get("output", []):
        if not isinstance(item, dict):
            continue
        for content in item.get("content", []):
            if not isinstance(content, dict):
                continue
            text = content.get("text")
            if isinstance(text, str) and text.strip():
                chunks.append(text.strip())
                continue
            if isinstance(text, dict):
                value = text.get("value")
                if isinstance(value, str) and value.strip():
                    chunks.append(value.strip())
    return "\n\n".join(chunks).strip()


def generate_with_openai(prompt: str) -> str:
    api_key = os.environ.get("OPENAI_API_KEY", "").strip()
    if not api_key:
        raise RuntimeError("missing OPENAI_API_KEY")

    model = os.environ.get("OPENAI_RELEASE_NOTES_MODEL", "").strip() or DEFAULT_MODEL
    payload = {
        "model": model,
        "max_output_tokens": 800,
        "input": [
            {
                "role": "system",
                "content": [
                    {
                        "type": "input_text",
                        "text": "You write sharp software release notes. Stick to the provided facts.",
                    }
                ],
            },
            {
                "role": "user",
                "content": [{"type": "input_text", "text": prompt}],
            },
        ],
    }
    request = urllib.request.Request(
        "https://api.openai.com/v1/responses",
        data=json.dumps(payload).encode("utf-8"),
        method="POST",
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            parsed = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"OpenAI API error: {exc.code} {body}") from None
    text = extract_response_text(parsed)
    if not text:
        raise RuntimeError("OpenAI response did not contain any text")
    return text.strip() + "\n"


def main() -> int:
    args = parse_args()
    current = args.current_tag.strip()
    repo = args.repo.strip() or detect_repo()
    prev = previous_tag(current)
    commits = commit_lines(prev, current)
    files = changed_files(prev, current)
    diff_summary = shortstat(prev, current)
    prompt = llm_prompt(
        repo=repo,
        start_tag=prev,
        end_tag=current,
        commit_items=commits,
        files=files,
        diff_summary=diff_summary,
    )
    output_path = Path(args.output).resolve()
    output_path.parent.mkdir(parents=True, exist_ok=True)

    try:
        notes = generate_with_openai(prompt)
    except Exception as exc:
        print(f"release-notes: falling back to deterministic summary ({exc})", file=sys.stderr)
        notes = fallback_notes(
            repo=repo,
            start_tag=prev,
            end_tag=current,
            commit_items=commits,
            files=files,
            diff_summary=diff_summary,
        )

    output_path.write_text(notes, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

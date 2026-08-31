---
title: "System overview"
subject: overview
keywords: [system, documentation, architecture, rzn-phone]
status: canonical
created: 2026-08-23
last_verified: 2026-08-23 @ 67b4d5c6387303ee1ad2077a0d95a83d25117453
read_when: "You need the top-level map of how this repository works."
skip_when: "You need one subsystem's source contract; read the named source file."
---

# System overview

This page is the top-level map for RZN Phone Automation. It is a local CLI and
MCP worker for a real iPhone. The worker uses Appium and XCUITest.

Read the pages in this order:

1. [Setup](setup.md) to prepare a Mac and an iPhone.
2. [Architecture](architecture.md) to follow the runtime path.
3. [Workflows](workflows.md) to add or change a workflow.
4. [Safety](safety.md) before any write action.
5. [CLI reference](cli.md) for the command surface.
6. [Testing and proof](testing.md) for checks and proof boundaries.
7. [Current canon](canon.md) for source authority.
8. [Contributing](contributing.md) for the change and check loop.

This folder is the current answer for the whole repository. Source code and
workflow data remain the authority for behavior.

<!-- tusker:docs-map:begin -->
```mermaid
graph TD
  n_architecture["Architecture"]
  n_canon["Current canon"]
  n_cli["CLI reference"]
  n_contributing["Contributing"]
  n_overview["System overview"]
  n_safety["Safety"]
  n_setup["Setup"]
  n_testing["Testing and proof"]
  n_workflows["Workflows"]
  n_overview --> n_architecture
  n_overview --> n_canon
  n_overview --> n_cli
  n_overview --> n_contributing
  n_overview --> n_safety
  n_overview --> n_setup
  n_overview --> n_testing
  n_overview --> n_workflows
```
<!-- tusker:docs-map:end -->

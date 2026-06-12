---
schema: "tusker.epic/v7"
kind: "epic"
id: "ARH"
project: "rzn-phone"
title: "Architecture review hardening"
status: "ready"
owner: "agent"
priority: "p0"
domains: []
next_task_number: 1
next_gate_number: 1
next_decision_number: 1
created_at: "2026-06-12T03:46:00Z"
updated_at: "2026-06-12T06:48:13Z"
state_rev: "sha256:10f10664d63b1070180431ce997ba513675e8b232c234f1b8e900af75778a164"
---

# ARH · Architecture review hardening

## Thesis

Senior architecture/security/performance review hardening backlog migrated from Beads on 2026-06-12.

## Success criteria

- [ ] P0 tasks close the live security, CLI contract, Appium lifecycle, and trace-bloat regressions.
- [ ] P1 tasks harden signed release verification, cold session timeouts, i18n extraction, tester/plugin packaging, MCP contract metadata, and CI/release permissions.
- [ ] P2 tasks clean lower-risk runtime, docs/help, and Cargo packaging hygiene.
- [ ] Stale review findings remain documented as rejected: clean git status, tracked LICENSE, old tracker state removed, and no README private absolute path in current HEAD.

## Current decision

Execute in waves:

| Wave | Parallel tasks | Notes |
|---|---|---|
| 1 | ARH-T-0001, ARH-T-0002, ARH-T-0003, ARH-T-0004, ARH-T-0005 | Independent P0 fixes. |
| 2 | ARH-T-0006 after ARH-T-0001; ARH-T-0007 after ARH-T-0004; ARH-T-0008, ARH-T-0009, ARH-T-0010, ARH-T-0011 in parallel | Security/signing and runtime follow-ups. |
| 3 | ARH-T-0012 after ARH-T-0005; ARH-T-0013 after ARH-T-0003; ARH-T-0014 anytime | Hygiene/docs/package cleanup. |

## Open gates

<!-- tusker:generated open-gates -->

| Gate | Owner | Blocks | Action |
|---|---|---|---|
| _None._ |  |  |  |

## Active work

<!-- tusker:generated active-work -->

| Task | Status | Next owner | Next action |
|---|---|---|---|
| [[ARH-T-0001]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0002]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0003]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0004]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0005]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0006]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0007]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0008]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0009]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0010]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0011]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0012]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0013]] | review | reviewer | Review evidence and close or return to rework. |
| [[ARH-T-0014]] | review | reviewer | Review evidence and close or return to rework. |

## Recently completed

<!-- tusker:generated recently-completed -->

| Task | Accepted by | Closed at |
|---|---|---|
| _None._ |  | |

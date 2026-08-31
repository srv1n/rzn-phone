---
schema: "tusker.domain/v7"
kind: "domain"
id: "runtime"
project: "rzn-phone"
title: "Runtime"
status: "current"
summary: "Rust worker, CLI, MCP, device session, and local state."
capsule:
  skip_when: "Skip when another domain is narrower or task proof/gates are the target."
  use_when: "Use when a task touches runtime behavior or needs the domain reading order."
  what: "Domain index for Runtime; routes agents to canon and owned knowledge files."
source_of_truth:
  - "knowledge/domains/runtime/CANON.md"
canonical_files:
  - "INDEX.md"
  - "CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T11:03:42Z"
state_rev: "sha256:d1cde38349b81096738bc624a6130403c9573f7d7fe311238fae8d5dc9f23a11"
---

# Runtime

## Summary

Rust worker, CLI, MCP, device session, and local state.

## Read This When

- You need current source-of-truth context for runtime.
- You are changing behavior owned by this domain.

## Canonical Files

- CANON.md - current durable truth.
- INDEX.md - domain map and routing hints.

## Runbooks

- _None yet._

## Interfaces

- _No stable interfaces declared yet._

## Invariants

- Keep durable truth in CANON.md.
- Put procedural guidance in runbooks/.

## Sources

- Raw external input belongs in sources/. Do not treat root docs/ or site output as canonical V7 knowledge.

## Glossary

- See glossary.md.

## Current Work

- _No current work linked._

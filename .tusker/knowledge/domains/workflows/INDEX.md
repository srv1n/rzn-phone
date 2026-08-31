---
schema: "tusker.domain/v7"
kind: "domain"
id: "workflows"
project: "rzn-phone"
title: "Workflows"
status: "current"
summary: "Workflow JSON contracts, catalog, safety, and authoring."
capsule:
  skip_when: "Skip when another domain is narrower or task proof/gates are the target."
  use_when: "Use when a task touches workflows behavior or needs the domain reading order."
  what: "Domain index for Workflows; routes agents to canon and owned knowledge files."
source_of_truth:
  - "knowledge/domains/workflows/CANON.md"
canonical_files:
  - "INDEX.md"
  - "CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T11:03:42Z"
state_rev: "sha256:167bd566ee9d253678fddb794ef193b12898e96fcec6b153e7a7afaaea9b0d8c"
---

# Workflows

## Summary

Workflow JSON contracts, catalog, safety, and authoring.

## Read This When

- You need current source-of-truth context for workflows.
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

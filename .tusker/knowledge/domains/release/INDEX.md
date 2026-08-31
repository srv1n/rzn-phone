---
schema: "tusker.domain/v7"
kind: "domain"
id: "release"
project: "rzn-phone"
title: "Release"
status: "current"
summary: "Build, test, package, sign, install, and publish paths."
capsule:
  skip_when: "Skip when another domain is narrower or task proof/gates are the target."
  use_when: "Use when a task touches release behavior or needs the domain reading order."
  what: "Domain index for Release; routes agents to canon and owned knowledge files."
source_of_truth:
  - "knowledge/domains/release/CANON.md"
canonical_files:
  - "INDEX.md"
  - "CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T11:03:42Z"
state_rev: "sha256:55145d25ca12759e427ed00d7a31eed5e4b79f143f74327d2e523e5ad5d58b62"
---

# Release

## Summary

Build, test, package, sign, install, and publish paths.

## Read This When

- You need current source-of-truth context for release.
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

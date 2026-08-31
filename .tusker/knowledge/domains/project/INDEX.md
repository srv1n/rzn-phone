---
schema: "tusker.domain/v7"
kind: "domain"
id: "project"
project: "rzn-phone"
title: "Project"
status: "current"
summary: "Durable project knowledge."
capsule:
  skip_when: "Skip when another domain is narrower or task proof/gates are the target."
  use_when: "Use when a task touches project behavior or needs the domain reading order."
  what: "Domain index for Project; routes agents to canon and owned knowledge files."
source_of_truth:
  - "knowledge/domains/project/CANON.md"
canonical_files:
  - "INDEX.md"
  - "CANON.md"
created_at: "2026-08-23T10:55:07Z"
updated_at: "2026-08-23T11:03:42Z"
state_rev: "sha256:d11580b79703949db1b6a82015be2d53268b118eccdf81c467620384638e505b"
---

# Project

## Summary

Durable project knowledge.

## Read This When

- You need current source-of-truth context for project.
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

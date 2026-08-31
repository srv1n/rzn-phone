---
schema: "tusker.domain/v7"
kind: "domain"
id: "product"
project: "rzn-phone"
title: "Product"
status: "current"
summary: "User-facing purpose, limits, and supported surfaces."
capsule:
  skip_when: "Skip when another domain is narrower or task proof/gates are the target."
  use_when: "Use when a task touches product behavior or needs the domain reading order."
  what: "Domain index for Product; routes agents to canon and owned knowledge files."
source_of_truth:
  - "knowledge/domains/product/CANON.md"
canonical_files:
  - "INDEX.md"
  - "CANON.md"
created_at: "2026-08-23T10:56:37Z"
updated_at: "2026-08-23T11:03:42Z"
state_rev: "sha256:3c87ce3cba7610d612bc5430724bd628a7827ed5624acb7851a8a04d77404bc7"
---

# Product

## Summary

User-facing purpose, limits, and supported surfaces.

## Read This When

- You need current source-of-truth context for product.
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

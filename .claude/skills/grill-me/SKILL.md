---
name: grill-me
description: Interview the user relentlessly about a plan or design until reaching shared understanding, resolving each branch of the decision tree. Use when the user wants to stress-test a plan, get grilled on their design, or mentions "grill me".
---

Interview me relentlessly about every aspect of this plan until we reach a shared understanding. Walk down each branch of the design tree, resolving dependencies between decisions one-by-one. For each question, provide your recommended answer first, with reasoning.

Ask the questions one at a time.

If a question can be answered by exploring the codebase, explore the codebase instead of asking. This repo is a Rust + nix + React/TanStack plugin-driven monolith ("Junius"); orient with `docs/design/`, `docs/impl/README.md`, and `docs/plugin-authoring-guide.md` when the design touches existing structure.

This is the shared interview discipline behind `create-feature-prd` and `create-capability-prd`; on its own it just stress-tests a plan and produces no artifact.

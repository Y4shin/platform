# Junius Plugin Development Guide

A step-by-step [mdBook](https://rust-lang.github.io/mdBook/) guide to building
plugins for the Junius platform. The rendered book lives in `src/`; start at
[`src/introduction.md`](src/introduction.md).

## Building locally

The toolchain does not ship `mdbook` yet (it is planned alongside the M21
documentation site). Until then, run it from `nixpkgs` ad hoc:

```bash
# Serve with live-reload at http://localhost:3000
nix run nixpkgs#mdbook -- serve docs/plugin-guide --open

# One-off build into docs/plugin-guide/book/
nix run nixpkgs#mdbook -- build docs/plugin-guide
```

If you have `mdbook` on your `PATH` already, `mdbook serve` / `mdbook build`
from inside `docs/plugin-guide/` work the same way.

## Scope

This book is the **narrative, tutorial-style** counterpart to the reference
material under [`../design/`](../design/) (architecture) and
[`../impl/`](../impl/) (per-milestone notes). It walks the `events` plugin
(`plugins/events/`) end to end as the worked example — every snippet has a real
counterpart in the tree. When the two disagree, the code wins: tell us so we can
fix the book.

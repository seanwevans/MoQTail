# RFCs — Request for Comments

> **Goal:** Document major design proposals before they are implemented.

The `rfcs/` directory hosts design documents that refine new features, large
refactors, or governance changes.  Each RFC starts life as a pull request.
Once merged, the RFC serves as a historical record of the decision.

## When to write an RFC

* Significant new functionality or APIs.
* Changes that affect interoperability or security.
* Any proposal that alters the project governance or release cadence.

Minor bug fixes and routine documentation updates do **not** require an RFC.

## Workflow summary

1. Start a discussion issue outlining the idea.
2. Copy [`template.md`](template.md) to `rfcs/NNN-my-title.md` on a feature branch.
3. Fill in the template and open a pull request targeting `main`.
4. The community reviews the proposal; revisions happen in the PR.
5. Once approved, the PR is merged and the RFC is assigned a number.

Merged RFCs are immutable; follow‑up changes must go through a new RFC.


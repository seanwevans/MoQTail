# Security Policy — MoQTail

> **Project maturity:** Early-development preview.  Expect rapid change and potential vulnerabilities.  We appreciate responsible disclosure.

---

## Supported Versions

| Version               | Status        | Security Fixes | Notes                            |
| --------------------- | ------------- | -------------- | -------------------------------- |
| `main`                | *active*      | ✔︎             | Development branch; fast‑moving. |
| `v0.1.x` (unreleased) | *pre‑release* | ✔︎             | Topic‑selector alpha.            |
| `<older>`             | *none*        | ✘              | Unsupported; please upgrade.     |

Once MoQTail reaches **v1.0**, we will maintain at least the two latest minor versions (`1.N–1.(N-1)`) with critical security updates for 12 months.

---

## Reporting a Vulnerability

1. **Email `security@moqtail.dev`** with:

   * Detailed description of the issue.
   * Steps to reproduce (PoC).
   * Impact assessment (confidentiality, integrity, availability).
   * Proposed patch or mitigation if available.
2. Encrypt reports with our PGP key *(fingerprint TBD; will be published here and on keys.openpgp.org).*  Unencrypted reports are accepted but discouraged.
3. We will acknowledge receipt **within three (3) business days** and provide a tracking ID.

> **Please do *not* open public GitHub issues or discuss potential vulnerabilities in public fora until we coordinate disclosure.**

---

## Disclosure Process

| Phase                | Typical Duration | Actions                                                                                                               |
| -------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------- |
| **Triage**           | ≤ 7 days         | Confirm exploitability, assign CVE (via [RustSec Advisory DB](https://github.com/rustsec/advisory-db) if Rust crate). |
| **Fix & Validation** | 7–30 days        | Develop, test, and audit patch; run fuzz + regression tests.                                                          |
| **Pre‑release**      | ≤ 5 days         | Share patch & release notes with reporter, major downstream brokers, and Linux distros list under embargo.            |
| **Public Release**   | —                | Tag patched versions, publish advisory, merge RustSec / GHSA, announce on `@MoqtailDev` and Matrix `#announcements`.  |
| **Post‑mortem**      | ≤ 14 days        | Document root cause, lessons learned, preventive issues filed.                                                        |

We aim for coordinated disclosure **within 30 days**. Timeline can extend for complex issues; we will keep the reporter informed.

---

## Vulnerability Classes in Scope

| Class                                     | Example                                                                                              | Notes                                                            |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| **Memory safety**                         | Unsafe Rust misuse leading to UB, buffer overflow, use‑after‑free.                                   | All `unsafe` blocks are audited; Miri/ASAN used in CI.           |
| **Denial‑of‑Service**                     | Regex backtracking blow‑ups, unbounded window state, malicious payload causing O(N²) path expansion. | Mitigations: timeouts, state caps, regex‑DFA limits.             |
| **Privilege escalation / sandbox escape** | Broker plugin executing arbitrary code via crafted selector or payload.                              | Broker plugins run in restricted process/jail when supported.    |
| **Authentication / authorization bypass** | Selector trick to access data beyond role permissions.                                               | Will be covered in v0.5 policy engine.                           |
| **Information leakage**                   | Timing side‑channels, verbose error strings revealing internals.                                     | Errors are generic by default; debug diagnostics require opt‑in. |

Out‑of‑scope for now: vulnerabilities in **downstream brokers**, user‑land JSON parsers, or hardware.

---

## Security Development Practices

* **CI hardening**

  * `cargo audit` + `cargo supply‑chain` on every push.
  * Fuzzing corpus via `honggfuzz‑rs`; coverage reports gated.
* **Code review** — two‑reviewer rule on all `unsafe` changes.
* **Dependency hygiene** — automatic `Dependabot` PRs, semver‑pin critical deps.
* **Reproducible builds** — Dockerfile pinned to Alpine + exact Rust toolchain version.
* **Security RFCs** — major design changes flow through an RFC process with section on threat modelling.

---

## Contact

* Security team: `security@moqtail.dev`
* Public chat (non‑vuln questions): Matrix `#moqtail:matrix.org`
* Project lead (PGP): *TBD*

---

© 2025 MoQTail Project Authors — Licensed under MIT & Apache‑2.0

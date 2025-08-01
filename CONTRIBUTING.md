# Contributing to **MoQTail**

First off, thank you for taking the time to contribute!  ❤️

This guide explains the project layout, coding standards, and pull‑request workflow so your contributions can be reviewed and merged quickly.

> **TL;DR:** Fork → feature branch → `cargo fmt` + tests → PR → review → merge.

---

## 1. How the Repository is Organised

| Path                   | What lives here                          | Language    |
| ---------------------- | ---------------------------------------- | ----------- |
| `crates/moqtail-core/` | DSL parser, AST, matcher engine          | Rust        |
| `crates/moqtail-cli/`  | Reference CLI (`moqtail sub`)            | Rust        |
| `plugins/`             | Broker plugins (Mosquitto, EMQX, HiveMQ) | Rust/C/Java |
| `docs/`                | mdBook user guide & DSL reference        | Markdown    |

> **Tip:** Run `cargo xtask repo-graph` to visualise crate dependencies (requires `cargo‑hack`).
>
> The alias in `.cargo/config.toml` lets you run it from the repository root:
>
> ```bash
> cargo xtask repo-graph
> ```
>
> It prints one `crate -> dependency` line for every workspace crate using `cargo metadata`.

---

## 2. Development Environment

### Prerequisites

* **Rust** 1.78+ (install via [`rustup`](https://rustup.rs)).
* **Node.js** 20+ (only if you hack on the JS bindings).
* **Python** 3.12+ with development headers (only for the optional Python bindings).
* **GNU Make** (used by `Makefile` shortcuts).
* **Docker** (optional, for broker plugin testing).
> **Tip:** On Debian/Ubuntu install the Python development headers with `sudo apt-get install python3-dev`. If you maintain multiple Python versions, set `PYO3_PYTHON=$(which python3)` before building the bindings.

> **Note:** The `bindings/python` crate depends on PyO3 and isn't built by default.
> CI and the default pre-commit hook skip it via `--workspace --exclude moqtail-python`.
> Enable it only when the Python prerequisites above are installed.

### Setup

```bash
# 1. Fork + clone
$ git clone https://github.com/<you>/moqtail.git && cd moqtail

# 2. Install toolchain & components
$ rustup override set stable
$ rustup component add clippy rustfmt

# 3. Run the full test suite
$ make check  # fmt + clippy + tests + docs links
```

`make check` is CI‑equivalent; your PR should pass it locally before you push.

---

## 3. Coding Standards

* **Formatting**: `cargo fmt` with default style **MUST** run clean.
* **Linting**: `cargo clippy --all-targets --all-features -- -D warnings` must be green.
* **Unsafe Rust**: Requires a *justification comment* and at least **two reviewers**.
* **Docs**: Public functions **SHOULD** have `///` examples that compile under `cargo test --doc`.
* **Commit Messages**: Conventional Commits style (`feat:`, `fix:`, `docs:`, `refactor:`…).

---

## 4. Pull‑Request Workflow

1. **Create a feature branch** off `main`:

   ```bash
   git checkout -b feat/dsl-regex
   ```
2. Make your changes. Keep PRs focused; small is beautiful.
3. Ensure `make check` passes.
4. Open a PR **against `main`**. Fill in the template:

   * **What & Why** (1–2 sentences).
   * **How** (key design choices).
   * **Testing** (manual steps or unit tests).
5. One core maintainer review is required; two for `unsafe` or security‑sensitive code.
6. **Squash‑merge** strategy. Maintainers may reword commits for clarity.

---

## 5. Filing Issues & RFCs

* **Bug report**: Use the *Bug* template. Provide a minimal reproducer; attach broker logs if relevant.
* **Feature request**: Start with *Discussion* → once fleshed out, open an *RFC* PR in `rfcs/`.
* **Security issue**: **DO NOT** file a public issue. Follow `SECURITY.md`.

---

## 6. Git Hooks (Optional but Recommended)

Run the helper once after cloning:

```bash
$ ./scripts/install-git-hooks.sh
```

It symlinks (or copies if necessary) every file in `scripts/hooks/` to
`.git/hooks/`. The default `pre-commit` hook runs `cargo fmt`, `cargo clippy`,
and `cargo test --workspace --exclude moqtail-python --quiet` so issues are
caught before you push.

---

## 7. Community Etiquette

We abide by the **Contributor Covenant**. See our [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). Be respectful and patient; we’re all volunteers.

If you have questions, join the Matrix room `#moqtail:matrix.org`.

---

## 8. License

By contributing, you agree that your work is licensed under the **MIT OR Apache‑2.0** dual license, unless explicitly stated otherwise in the PR.

Happy hacking! 🦀

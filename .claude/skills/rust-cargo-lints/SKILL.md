---
name: rust-cargo-lints
description: "The backend/ Cargo workspace: members, [workspace.package], [workspace.dependencies], [workspace.lints] (pedantic plus restriction picks), clippy.toml, CI lint and fmt commands, rustfmt, and the release profile for the Docker server. Use when editing any Cargo.toml, lint or profile."
---

# Cargo workspace, lints, rustfmt and the release profile

This skill sets up `backend/`: the workspace manifest, the lint set every crate inherits, the CI commands, rustfmt and the release profile. The layout comes from [backend README 6](../../../docs/backend/README.md#6-workspace-layout) (which refines [architecture 13](../../../docs/architecture/README.md#13-repository-layout)), the crate graph from [backend README 5](../../../docs/backend/README.md#5-crate-graph-and-dependency-rules), the lint rules from [README 7](../../../docs/backend/README.md#7-rust-conventions) and [11](../../../docs/backend/README.md#11-testing-and-tooling), and the release profile from [README 12](../../../docs/backend/README.md#12-builds-and-deployment); **the project documents win** over anything here. Versions come from [`../rust/versions.md`](../rust/versions.md). The backend is a long-running server in Docker, not a crates.io library ([architecture 12](../../../docs/architecture/README.md#12-deployment)), so library-only rules (MSRV policy, `missing_docs`, `#[non_exhaustive]`) are left out on purpose.

Claims marked ✓ were run on 2026-10-15 with Rust 1.99.1 (Cargo 1.99.1, Clippy 0.1.99, rustfmt 1.10.0), on macOS and, for the profile, in `rust:1.99.1-slim` (Linux).

## 1. Layout and the root manifest

```
backend/
├─ Cargo.toml            the manifest below (virtual: no [package])
├─ Cargo.lock            committed: the binaries are deployed
├─ rust-toolchain.toml   1.99.1 with rustfmt and clippy
├─ clippy.toml
├─ rustfmt.toml          this skill's addition (section 5)
├─ .config/nextest.toml  this skill's addition (rust-testing)
├─ crates/{domain, store, blob, integrations, vision, knowledge, agent, pipeline}/
├─ bins/{api, worker}/
└─ migrations/schema.surql  not a package, so outside `members`
```

The crates are those of [backend README 5](../../../docs/backend/README.md#5-crate-graph-and-dependency-rules): architecture 3's seven plus `integrations`, one crate for every outbound HTTP adapter. README 6 lists the files above except `rustfmt.toml` and `.config/nextest.toml`, which this skill and [rust-testing](../rust-testing/SKILL.md) add.

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.99.1"
components = ["rustfmt", "clippy"]
```

`rustup` reads this file in the directory and its parents, so every `cargo` run in `backend/` uses 1.99.1 and installs it if missing ([rustup, the toolchain file](https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file)). Bump it with `rust-version` and the Rust row in [versions.md](../rust/versions.md#toolchain).

The block below is the root manifest. `[workspace.dependencies]` has one row per crate row in all three tables of [versions.md](../rust/versions.md), with its features ([backend README 4](../../../docs/backend/README.md#4-stack-and-version-pins)). The [example checker](../rust/scripts/check_examples.py) reads its `[workspace.lints]` tables as the `[lints]` of every skill example, so every example in the `rust-*` skills passes exactly this set. Don't change a level here without running the checker on every skill.

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["crates/*", "bins/*"]
resolver = "3"            # a virtual workspace must say it; edition 2024 implies "3"

[workspace.package]
edition = "2024"
rust-version = "1.99"     # the toolchain we build with, not a support promise
version = "0.1.0"
publish = false

[workspace.dependencies]
# Our crates
domain = { path = "crates/domain" }
store = { path = "crates/store" }
blob = { path = "crates/blob" }
integrations = { path = "crates/integrations" }
vision = { path = "crates/vision" }
knowledge = { path = "crates/knowledge" }
agent = { path = "crates/agent" }
pipeline = { path = "crates/pipeline" }
# versions.md, dependencies
tokio = { version = "1.53.3", features = ["full"] }
tokio-util = { version = "0.7.20", features = ["rt"] }
tokio-stream = { version = "0.1.19", features = ["sync"] }
futures = "0.3.34"
async-trait = "0.1.92"
async-stream = "0.3.6"
axum = { version = "0.8.9", features = ["macros", "multipart"] }
tower = { version = "0.5.3", features = ["util", "timeout", "limit"] }
tower-http = { version = "0.7.1", features = ["trace", "catch-panic", "cors", "limit", "timeout", "request-id", "util", "compression-gzip", "compression-br"] }
http-body-util = "0.1.5"
governor = "0.10.4"
jsonwebtoken = { version = "11.1.0", default-features = false, features = ["aws_lc_rs", "use_pem"] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = { version = "1.0.154", features = ["preserve_order"] }
utoipa = { version = "6.0.0", features = ["axum_extras", "chrono"] }
utoipa-axum = "0.3.0"
schemars = { version = "1.2.3", features = ["chrono04"] }
chrono = { version = "0.4.45", default-features = false, features = ["std", "clock", "serde"] }
ulid = { version = "3.0.0", features = ["serde"] }
uuid = { version = "1.28.0", features = ["v4", "serde"] }
thiserror = "2.0.21"
anyhow = "1.0.104"        # binaries only (README 7)
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter", "json"] }
surrealdb = { version = "=3.3.4", default-features = false, features = ["protocol-ws", "rustls"] }  # exact: moves with the server image (database 1); store adds kv-mem for tests
aws-sdk-s3 = { version = "1.152.0", default-features = false, features = ["default-https-client", "rt-tokio", "http-1x"] }
reqwest = { version = "0.13.5", features = ["json", "stream", "query"] }
ort = { version = "=2.0.0-rc.14", default-features = false, features = ["std", "ndarray", "tracing"] }  # exact: a pre-release; vision's features add load-dynamic, api-29, cuda, migraphx, webgpu
ndarray = "0.17.2"
rand = "0.10.3"
serde_path_to_error = "0.1.20"
aws-lc-rs = "1.18.2"
base64 = "0.23.1"
# versions.md, pinned by the design
bytes = "1.12.1"
chrono-tz = "0.10.4"
secrecy = "0.10.3"
data-encoding = "2.11.1"
moka = { version = "0.12.16", features = ["future"] }
tzf-rs = { version = "2.1.3", default-features = false, features = ["bundled"] }
regex = "1.13.1"
hmac = "0.13.0"
sha2 = "0.11.1"
image = { version = "0.25.10", default-features = false, features = ["jpeg", "png", "webp"] }
imageproc = { version = "0.27.0", default-features = false }
fast_image_resize = { version = "6.1.0", features = ["image"] }
png = "0.18.1"
webp = { version = "0.3.1", default-features = false }
libheif-rs = { version = "3.0.0", default-features = false, features = ["v1_19"] }
nalgebra = "0.35.0"
chacha20 = { version = "0.10.2", default-features = false, features = ["rng"] }
tokenizers = { version = "0.23.2", default-features = false, features = ["fancy-regex"] }
toml = "1.1.8"
clap = { version = "4.6.7", features = ["derive", "env"] }
# Dev-dependencies are listed here too; members add test-util (tokio) and kv-mem (surrealdb)
insta = { version = "1.49.0", features = ["json", "redactions"] }
proptest = "1.12.0"
mockall = "0.15.0"
wiremock = "0.6.5"
svix = "2.8.0"            # test oracle only (README 4)

[workspace.lints.rust]
unsafe_code = "deny"                                   # allow-by-default
rust_2018_idioms = { level = "warn", priority = -1 }   # group of allow-by-default lints

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }   # group, allow by default
missing_errors_doc = "allow"                   # pedantic; library docs
missing_panics_doc = "allow"                   # pedantic; library docs
must_use_candidate = "allow"                   # pedantic; library API advice
unwrap_used = "warn"                           # restriction
dbg_macro = "warn"                             # restriction
todo = "warn"                                  # restriction
print_stdout = "warn"                          # restriction
print_stderr = "warn"                          # restriction
allow_attributes_without_reason = "warn"       # restriction

[profile.release]               # backend README 12, every key
lto = "thin"
codegen-units = 1
panic = "unwind"                # the default; never "abort" (section 6)
debug = "line-tables-only"      # file:line in panic backtraces
overflow-checks = true          # panic instead of wrapping
# no `strip`: stripped symbols empty a panic's backtrace (section 6)
```

```toml
# clippy.toml
allow-unwrap-in-tests = true
# This skill's addition to README 6's clippy.toml: await_holding_invalid_type (suspicious, warn by default) reads this list; see rust-tracing's json-logs.md
await-holding-invalid-types = [
  { path = "tracing::span::Entered", reason = "a span guard across .await gives wrong traces; use tracing::Instrument" },
  { path = "tracing::span::EnteredSpan", reason = "a span guard across .await gives wrong traces; use tracing::Instrument" },
]
```

`await-holding-invalid-types` makes Clippy warn when a `span.enter()` guard is held across `.await` ([`await_holding_invalid_type`](https://clippy.rust-lang.org/rust-1.99.0/index.html#await_holding_invalid_type); [rust-tracing, json-logs.md](../rust-tracing/json-logs.md#4-spans-across-tokiospawn-and-spawn_blocking)).

Workspace rules, each checked on a probe workspace with this exact layout:

- **`members` globs.** `members = ["crates/*", "bins/*"]` makes every directory under `crates/` and `bins/` a member, so a new crate needs no manifest edit. Path dependencies inside the workspace become members anyway. Members "should be … directories with Cargo.toml files", and `exclude` removes a directory that a glob matches ([Cargo, members and exclude](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-members-and-exclude-fields)). ✓ An empty `crates/empty/` stops the whole workspace from loading (`failed to load manifest for workspace member … failed to read crates/empty/Cargo.toml`), so every `cargo` command fails until the directory is removed or listed in `exclude`; a plain file such as `crates/notes.md` is skipped. Keep only packages directly under `crates/` and `bins/`.
- **`resolver = "3"` is required.** A virtual manifest has no `package.edition` to infer it from ([Cargo, virtual workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html#virtual-workspace)). ✓ Without it Cargo warns `virtual workspace defaulting to resolver = "1" despite one or more workspace members being on edition 2024 which implies resolver = "3"`. Resolver 3 makes `resolver.incompatible-rust-versions = "fallback"` the default, so dependency versions are picked to suit `rust-version` ([Cargo, resolver versions](https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions); [edition guide](https://doc.rust-lang.org/edition-guide/rust-2024/cargo-resolver.html)). ✓ `cargo` then prints `Locking 51 packages to highest Rust 1.99 compatible versions`.
- **`rust-version` documents the toolchain.** Cargo refuses to build with an older toolchain, and the resolver and Clippy's `incompatible_msrv` read it ([Cargo, rust-version uses](https://doc.rust-lang.org/cargo/reference/rust-version.html#uses); [`incompatible_msrv`](https://clippy.rust-lang.org/rust-1.99.0/index.html#incompatible_msrv), suspicious, warn). Bump it in the same commit as the toolchain (the Rust row in versions.md and the build image). There is no MSRV policy.
- **Members inherit with `key.workspace = true`** for `edition`, `rust-version`, `version` and `publish` ([Cargo, package table](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-package-table)). `profile.*` is read only from the root manifest ([Cargo, workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html#workspaces)).

## 2. Dependencies

A member names each dependency with `workspace = true` and may add features:

```toml
# backend/bins/api/Cargo.toml
[package]
name = "api"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[dependencies]
domain.workspace = true
store.workspace = true
tokio.workspace = true
axum.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
insta.workspace = true

[lints]
workspace = true
```

- **Features add up.** Features on the member are added to the workspace row's; `optional` can't be set in `[workspace.dependencies]` ([Cargo, workspace dependencies](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table); [inheriting](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#inheriting-a-dependency-from-a-workspace)). ✓ A member's `features = ["rc"]` on `serde` shows up in `cargo tree -e features`.
- **`default-features = false` on a member now works.** In edition 2024 packages on Rust 1.99+, a member's `default-features` overrides the workspace row ([Cargo, workspace dependencies](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table)). ✓ With the workspace row on defaults and the member at `default-features = false`, `cargo tree -e features` lists `serde feature "derive"` and no `"default"`. Code written for older Cargo set `default-features = false` in the workspace row instead; both work now.
- **`test-util` stays out of release builds.** Features enabled on a dev-dependency aren't unified into the normal build unless a test or example is being built ([Cargo, feature resolver 2](https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2)), so `tokio`'s paused clock never reaches the `api` binary.
- **Mapping versions.md onto the table.** Each crate row becomes one `[workspace.dependencies]` row with its features. Write the version as a plain requirement (`"1.53.3"` means `>=1.53.3, <2.0.0`, [Cargo, caret](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#caret-requirements)); the committed `Cargo.lock` holds the exact versions and `--locked` makes CI fail if it would change ([Cargo FAQ, Cargo.lock](https://doc.rust-lang.org/cargo/faq.html#why-have-cargolock-in-version-control); [`--locked`](https://doc.rust-lang.org/cargo/commands/cargo-build.html#option-cargo-build---locked)). Two rows are exact (`=`, [Cargo, comparison](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#comparison-requirements)): `surrealdb`, because the crate and the server move together ([database 1](../../../docs/database/README.md#1-version-and-setup)), and `ort`, because a pre-release requirement also accepts later pre-releases of the same version ([Cargo, pre-releases](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#pre-releases)). Moving a version means editing versions.md and this table in one commit, then `cargo update -p <crate>`.

## 3. Lints

### Syntax and priority

- `[workspace.lints.rust]` and `[workspace.lints.clippy]` take `lint = "level"` or `{ level, priority }`; the table is the part of the lint name before `::` ([Cargo, `[lints]`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section); [workspace lints](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table)). Stable since Rust 1.74 ([release notes 1.74.0](https://doc.rust-lang.org/releases.html#version-1740-2023-11-16)).
- **Groups get `priority = -1`.** Cargo ignores the order of keys in the table; lower priorities are passed first, so later, more specific lints win ([Cargo, `[lints]`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)). ✓ `cargo clippy -v` shows `--warn=rust_2018_idioms` and `--warn=clippy::pedantic` before `--allow=clippy::must_use_candidate`. A group left at priority 0 next to a lint is an error from [`lint_groups_priority`](https://clippy.rust-lang.org/rust-1.99.0/index.html#lint_groups_priority) (correctness, deny). ✓ Its "known problems" say it skips inherited lints, but on 1.99 it fired for `[workspace.lints]`: `error: lint group pedantic has the same priority (0) as a lint`.
- **Members opt in with `[lints] workspace = true`** and can't override single lints there ([Cargo, workspace lints](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table)). ✓ Adding `[lints.clippy] unwrap_used = "allow"` (or `clippy = { … }` inline) next to `workspace = true` fails to load the manifest:

  ```
  cannot override `workspace.lints` in `lints`, either remove the overrides or `lints.workspace = true` and manually specify the lints
  ```

- **The escape hatch is one item.** Put `#[expect(lint, reason = "…")]` on the narrowest item; prefer it to `#[allow]`, because an expectation that is no longer met warns with `unfulfilled_lint_expectations` ([Reference, `#[expect]`](https://doc.rust-lang.org/reference/attributes/diagnostics.html#the-expect-attribute); [lint reasons](https://doc.rust-lang.org/reference/attributes/diagnostics.html#lint-reasons)). Both stable since Rust 1.81 ([release notes 1.81.0](https://doc.rust-lang.org/releases.html#version-1810-2024-09-05)). ✓ `allow_attributes_without_reason` flags `#[expect(...)]` without a reason as well as `#[allow(...)]`.

```rust
use std::time::Duration;

/// `Retry-After` seconds for a 429 or 503 (contracts §2, `retry_after_s`).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "ceil() of a positive duration under one day fits u32"
)]
fn retry_after_s(wait: Duration) -> u32 {
    wait.as_secs_f64().ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_up() {
        assert_eq!(retry_after_s(Duration::from_millis(1_200)), 2);
    }
}
```

### The set

| Lint | Group, default | Level | Why |
|---|---|---|---|
| [`unsafe_code`](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html#unsafe-code) | rustc, allow | deny | Nothing in the backend needs `unsafe`. This skill's choice: `deny`, not `forbid`, so a reviewed `#[expect(unsafe_code, reason = …)]` stays possible; `forbid` can't be lowered ([lint levels](https://doc.rust-lang.org/rustc/lints/levels.html#forbid)) |
| [`rust_2018_idioms`](https://doc.rust-lang.org/rustc/lints/groups.html) | rustc group, allow | warn, −1 | In edition 2024 its `bare_trait_objects` and `...` ranges are already hard errors (✓ E0782, E0783), so it adds [`elided_lifetimes_in_paths`](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html#elided-lifetimes-in-paths) (write `Formatter<'_>`), [`explicit_outlives_requirements`](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html#explicit-outlives-requirements) and [`unused_extern_crates`](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html#unused-extern-crates) |
| [`pedantic`](https://clippy.rust-lang.org/rust-1.99.0/index.html) | clippy group, allow | warn, −1 | This skill's choice: casts, needless clones and [`large_futures`](https://clippy.rust-lang.org/rust-1.99.0/index.html#large_futures) (futures over 16 384 bytes) matter in an async server. ✓ It does not fire on plain axum handlers: `unused_async` stays quiet for `async fn healthz() -> &'static str` passed to `get()`, and `needless_pass_by_value` for `Json(body): Json<T>` |
| [`missing_errors_doc`](https://clippy.rust-lang.org/rust-1.99.0/index.html#missing_errors_doc), [`missing_panics_doc`](https://clippy.rust-lang.org/rust-1.99.0/index.html#missing_panics_doc) | pedantic, allow | allow | Rustdoc sections for library users; we have none |
| [`must_use_candidate`](https://clippy.rust-lang.org/rust-1.99.0/index.html#must_use_candidate) | pedantic, allow | allow | Library API advice; it would ask for `#[must_use]` on most functions |
| [`unwrap_used`](https://clippy.rust-lang.org/rust-1.99.0/index.html#unwrap_used) | restriction, allow | warn | No `unwrap()` outside tests; `expect("why it can't fail")` for invariants. `allow-unwrap-in-tests` lets `#[test]` functions and `#[cfg(test)]` modules use it |
| [`dbg_macro`](https://clippy.rust-lang.org/rust-1.99.0/index.html#dbg_macro) | restriction, allow | warn | Debug leftovers |
| [`todo`](https://clippy.rust-lang.org/rust-1.99.0/index.html#todo) | restriction, allow | warn | Unfinished code in a deploy |
| [`print_stdout`](https://clippy.rust-lang.org/rust-1.99.0/index.html#print_stdout), [`print_stderr`](https://clippy.rust-lang.org/rust-1.99.0/index.html#print_stderr) | restriction, allow | warn | Logs are JSON through `tracing` ([architecture 12, operations](../../../docs/architecture/README.md#operations)); a stray `println!` breaks the log stream |
| [`allow_attributes_without_reason`](https://clippy.rust-lang.org/rust-1.99.0/index.html#allow_attributes_without_reason) | restriction, allow | warn | Every suppression says why |

Already on by default and worth knowing in this code base: [`await_holding_lock`](https://clippy.rust-lang.org/rust-1.99.0/index.html#await_holding_lock) (suspicious, warn), [`let_underscore_future`](https://clippy.rust-lang.org/rust-1.99.0/index.html#let_underscore_future) (suspicious, warn) and [`result_large_err`](https://clippy.rust-lang.org/rust-1.99.0/index.html#result_large_err) (perf, warn, error types over 128 bytes; keep `ApiError` small, see [`rust-errors`](../rust-errors/SKILL.md)).

Left out on purpose:

- **Library-only.** [`missing_docs`](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html#missing-docs) and [`unreachable_pub`](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html#unreachable-pub) (rustc, allow), [`missing_docs_in_private_items`](https://clippy.rust-lang.org/rust-1.99.0/index.html#missing_docs_in_private_items), and the `#[non_exhaustive]` policy lints [`exhaustive_enums`](https://clippy.rust-lang.org/rust-1.99.0/index.html#exhaustive_enums) and [`exhaustive_structs`](https://clippy.rust-lang.org/rust-1.99.0/index.html#exhaustive_structs) (all restriction, allow). They protect downstream users of a published API; nothing outside the workspace links our crates, and contracts change by pull request ([AGENTS.md, sources of truth](../../../AGENTS.md#sources-of-truth)).
- **[`expect_used`](https://clippy.rust-lang.org/rust-1.99.0/index.html#expect_used)** (restriction, allow): `expect` with a reason is the house rule for invariants (see `unwrap_used` above).
- **[`module_name_repetitions`](https://clippy.rust-lang.org/rust-1.99.0/index.html#module_name_repetitions)** is restriction in 1.99, not pedantic, so `store::StoreError` is fine without an override.
- **`cargo` group** ([`multiple_crate_versions`](https://clippy.rust-lang.org/rust-1.99.0/index.html#multiple_crate_versions) and others): the tree has known duplicates (`tower-http` 0.6 inside `reqwest`, `ulid` 1.x inside `surrealdb-types`; [versions.md, compatibility](../rust/versions.md#compatibility-checks)).

The set is the one [backend README 7](../../../docs/backend/README.md#7-rust-conventions) describes ("`unsafe_code = "deny"`; clippy pedantic with a few allows; `unwrap_used` warns outside tests") and the one the backend design's compiled workspace uses, table for table.
- **`cargo deny` and `cargo audit`** aren't covered by the sources these skills may use, so they are not part of this setup.

## 4. CI commands

Run from `backend/`:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

- `--all-targets` lints tests, examples and benches too; `[lints]` applies to every target of a package ([Cargo, `[lints]`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)).
- **`-D warnings` and `[lints]` don't fight.** `warnings` is a special group that "applies to all lints that would otherwise warn" ([rustc, priority of lint sources](https://doc.rust-lang.org/rustc/lints/levels.html#priority-of-lint-level-sources)), so `allow` stays allow and `#[expect]` and `#[allow]` on items still win over the flag. ✓ With `-- -D warnings`, `must_use_candidate` stayed silent, an item with `#[allow(clippy::cast_possible_truncation, reason = …)]` passed, and a stale `#[expect]` became `error: this lint expectation is unfulfilled`.
- **The Cargo-native form** is `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets --locked` (`build.warnings`, respected since Cargo 1.97, [Cargo config](https://doc.rust-lang.org/cargo/reference/config.html#buildwarnings)). ✓ It fails with `warnings are denied by build.warnings configuration` and, unlike `-- -D warnings`, it didn't re-check the workspace crates after a plain `cargo clippy` (0 `Checking` lines against 2), because the rustc flags don't change. Either is fine; pick one for CI and the same locally.
- Dependencies never fail the build on their warnings: Cargo passes `--cap-lints allow` to them ([rustc, capping lints](https://doc.rust-lang.org/rustc/lints/levels.html#capping-lints)).

Tests in CI are in [`rust-testing`](../rust-testing/SKILL.md#7-running-tests-locally-and-in-ci).

## 5. rustfmt

```toml
# backend/rustfmt.toml
edition = "2024"
```

- **Default options only.** Use no unstable options: they need nightly ([rustfmt configuration](https://rust-lang.github.io/rustfmt/?version=v1.10.0)). The stable ones at rustfmt 1.10.0 are the widths (`max_width` 100 and the `*_width` heuristics), `hard_tabs`, `tab_spaces`, `newline_style`, `edition`, `style_edition`, `reorder_imports`, `reorder_modules`, `remove_nested_parens`, `merge_derives`, `use_field_init_shorthand`, `use_try_shorthand`, `force_explicit_abi`, `fn_params_layout`, `match_arm_leading_pipes`, `match_block_trailing_comma`, `hex_literal_case`, `short_array_element_width_threshold`, `single_line_if_else_max_width`, `single_line_let_else_max_width` and `disable_all_formatting`. We change none of the formatting ones. The rustfmt page is rendered by script: a link such as `?version=v1.10.0#edition` loads the page, and the script then scrolls to the option. The same text is in [`Configurations.md` at v1.10.0](https://github.com/rust-lang/rustfmt/blob/v1.10.0/Configurations.md).
- **Why the file exists.** `cargo fmt` takes the edition from `Cargo.toml` (✓ `cargo fmt -v` runs `rustfmt --edition 2024`), but `rustfmt` run directly, by an editor for example, parses as edition 2015 ([`edition`](https://rust-lang.github.io/rustfmt/?version=v1.10.0#edition)). ✓ Without the file, `rustfmt --check` on a file with an `async fn` fails with `E0670: async fn is not permitted in Rust 2015`. `style_edition` follows `edition` when it isn't set ([`style_edition`](https://rust-lang.github.io/rustfmt/?version=v1.10.0#style_edition)); ✓ with only `edition = "2024"`, rustfmt version-sorts `use x::{u10, u9}` as the 2024 style does. rustfmt also falls back to a `rustfmt.toml` in the home or global config directory when the project has none ([rustfmt configuration](https://rust-lang.github.io/rustfmt/?version=v1.10.0)), so a project file keeps personal settings out.
- `cargo fmt --check` exits 1 and prints the diff when a file isn't formatted. ✓ In a virtual workspace it covers every member without `--all`.

## 6. The release profile

[Backend README 12](../../../docs/backend/README.md#12-builds-and-deployment) (v0.2.1) fixes every key: `lto = "thin"`, `codegen-units = 1`, `panic = "unwind"`, `debug = "line-tables-only"`, `overflow-checks = true`, and no `strip`. Everything else stays at Cargo's release defaults (`opt-level = 3`, `incremental = false`; [Cargo, release](https://doc.rust-lang.org/cargo/reference/profiles.html#release)).

| Key | Value | Why |
|---|---|---|
| [`panic`](https://doc.rust-lang.org/cargo/reference/profiles.html#panic) | `"unwind"` (default) | Never `"abort"`, "so `CatchPanicLayer` and task joins can contain a panic" (README 12). tower-http's `CatchPanicLayer` turns a handler panic into a 500 with `std::panic::catch_unwind` (✓ `tower-http-0.7.1/src/catch_panic.rs`), tokio reports a panicked task as a `JoinError` whose [`is_panic()`](https://docs.rs/tokio/1.53.3/tokio/task/struct.JoinError.html#method.is_panic) is true, and `catch_unwind` "only catches unwinding panics, not those that abort the process" ([std, `catch_unwind`](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html)). ✓ With `panic = "abort"`, a panic inside `tokio::spawn` killed the process (exit 134) instead of returning `JoinError`. *Judgement*: the perf book's abort advice applies only when nothing catches panics ([perf book, abort on panic](https://nnethercote.github.io/perf-book/build-configuration.html#abort-on-panic)). `panic` and `lto` can't be set per package ([Cargo, overrides](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides)) |
| [`lto`](https://doc.rust-lang.org/cargo/reference/profiles.html#lto) | `"thin"` | README 12. "Thin" LTO works across all crates of the build at a fraction of fat LTO's link time ([Cargo, lto](https://doc.rust-lang.org/cargo/reference/profiles.html#lto)); *Judgement*: it "can improve runtime speed by 10-20% or more" at the cost of compile time ([perf book, LTO](https://nnethercote.github.io/perf-book/build-configuration.html#link-time-optimization)) |
| [`codegen-units`](https://doc.rust-lang.org/cargo/reference/profiles.html#codegen-units) | `1` | README 12. Fewer units let the compiler optimise more at the cost of build parallelism ([Cargo, codegen-units](https://doc.rust-lang.org/cargo/reference/profiles.html#codegen-units)); *Judgement*: [perf book, codegen units](https://nnethercote.github.io/perf-book/build-configuration.html#codegen-units). Both slow the release build, which the Docker images' dependency caching (README 12) offsets |
| [`debug`](https://doc.rust-lang.org/cargo/reference/profiles.html#debug) | `"line-tables-only"` | README 12: without line tables a backtrace's frames have function names but no file or line, because Cargo strips debug info from a release build that requests none; line tables add them, at the cost of a larger binary (README 12 measured a test binary going from 0.46 MB to 2.25 MB). ✓ On Linux the default release build printed `relprobe::deep` with no location, this one `deep at ./src/main.rs:5:17`. Since 1.77 Cargo passes `-C strip=debuginfo` when `debug` is off and stops when it is on ([release notes 1.77.0, Cargo](https://doc.rust-lang.org/releases.html#version-1770-2024-03-21)) |
| [`strip`](https://doc.rust-lang.org/cargo/reference/profiles.html#strip) | not set | README 12: `strip = "symbols"` removes the symbol table, and a panic's backtrace then has no frames at all (`RUST_BACKTRACE=full` shows only `<unknown>` addresses, probed by the design on Linux and macOS); `"debuginfo"` would remove the line tables. The panic message's own `file:line:col` comes from the panic's [`Location`](https://doc.rust-lang.org/std/panic/struct.Location.html), not from debug info, so it is kept either way |
| [`overflow-checks`](https://doc.rust-lang.org/cargo/reference/profiles.html#overflow-checks) | `true` | README 12. Off by default in release, where `+`, `-` and `*` wrap silently; on, they panic ([Reference, overflow](https://doc.rust-lang.org/reference/expressions/operator-expr.html#overflow)). ✓ `254u8 + 3` printed `1` by default and panicked with `attempt to add with overflow` when on. A panic becomes a 500 or a failed task, while a wrapped `seq`, attempt count or byte size is silent corruption. The cost is a check and branch per arithmetic operation, not measured here; if a profile shows a hot loop (the blur check or depth processing in `vision`), use `wrapping_*` there or a [package override](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides) for that crate |

- **`split-debuginfo`.** Leave it unset. The Linux default keeps the line tables inside the binary; `unpacked` is the default only on macOS ([Cargo, split-debuginfo](https://doc.rust-lang.org/cargo/reference/profiles.html#split-debuginfo)), and a split file would have to be copied into the image.
- **`opt-level`.** Keep 3. `"s"` and `"z"` are for binary size, which doesn't matter in a server image ([Cargo, opt-level](https://doc.rust-lang.org/cargo/reference/profiles.html#opt-level)).
- **Backtraces at run time.** Line tables only help if a backtrace is captured. Set `RUST_BACKTRACE=1` and `RUST_LIB_BACKTRACE=0` in the container: the default panic hook prints a backtrace, while `Backtrace::capture` (which `anyhow` uses for every error) stays off, because capturing is "a quite expensive runtime operation" ([std, backtrace environment variables](https://doc.rust-lang.org/std/backtrace/index.html#environment-variables); [anyhow, details](https://docs.rs/anyhow/1.0.104/anyhow/index.html#details)). The variables are read once and cached.

## Review checklist

- [ ] A new crate is under `crates/` or `bins/`, fits the crate graph of [backend README 5](../../../docs/backend/README.md#5-crate-graph-and-dependency-rules), inherits `edition`, `rust-version`, `version` and `publish`, and has `[lints] workspace = true`.
- [ ] Every dependency is `name.workspace = true`, with a row in versions.md and in `[workspace.dependencies]`; extra features are added at the member, never a second version.
- [ ] No lint override in a member's `Cargo.toml`, no crate-level `#![allow]`; each suppression is an item-level `#[expect(..., reason = "...")]`.
- [ ] `Cargo.lock` is committed and CI uses `--locked`; `rust-toolchain.toml` pins 1.99.1 with `rustfmt` and `clippy`.
- [ ] `cargo fmt --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- [ ] `[profile.release]` is README 12's: `lto = "thin"`, `codegen-units = 1`, `panic = "unwind"`, `debug = "line-tables-only"`, `overflow-checks = true`, no `strip`.

## Related skills

[`rust`](../rust/SKILL.md) (entry, versions, checker) · [`rust-testing`](../rust-testing/SKILL.md) (nextest config, test commands) · [`rust-errors`](../rust-errors/SKILL.md) (panics, `ApiError` size) · [`rust-axum-api`](../rust-axum-api/SKILL.md) (`CatchPanicLayer`) · [`rust-async`](../rust-async/SKILL.md) (`JoinError`) · [`rust-tracing`](../rust-tracing/SKILL.md) (JSON logs) · [`rust-surrealdb`](../rust-surrealdb/SKILL.md) · [`rust-workers`](../rust-workers/SKILL.md) (`ort` pin) · [`rust-contract-types`](../rust-contract-types/SKILL.md)

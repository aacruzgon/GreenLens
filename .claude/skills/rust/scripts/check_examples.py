#!/usr/bin/env python3
"""Compile every Rust example in the rust* skills.

Extracts each ```rust block from .claude/skills/rust*/**/*.md, writes the blocks into a
throwaway Cargo crate in a temporary directory outside the repository, and runs, in order:

  1. the copy check: every region marked `// copy of: <file> <Name>` must equal the region
     marked `// canonical: <file> <Name>`, line for line;
  2. rustfmt --check on every compiled block (edition 2024, default options, an empty
     rustfmt.toml so no user or parent configuration applies);
  3. cargo check --keep-going on every ```rust block (as an example, or as an integration
     test when it holds tests);
  4. cargo clippy --keep-going on the same targets, with the [lints] table and clippy.toml
     taken from .claude/skills/rust-cargo-lints/SKILL.md;
  5. cargo check on every ```rust,compile_fail block, each of which must fail; with an
     error code (```rust,compile_fail,E0277) every error must carry exactly that code;
  6. cargo test on every test block, one target at a time (INSTA_UPDATE=no, CI=true),
     counting tests run, passed and ignored;
  7. with --live: the ignored SurrealDB tests, under `cargo nextest run --run-ignored only`,
     against the pinned surrealdb/surrealdb image (by digest, from docs/database section 1)
     in a container that is stopped afterwards.

Dependency versions come from the two crate tables in .claude/skills/rust/versions.md.
Any warning or error in a block fails the run. There is no baseline of accepted failures.

Fences:
  ```rust                      compiled. A block with no `fn main` gets `fn main() {}` appended.
                               A block with #[test], #[tokio::test], #[cfg(test)] or proptest!
                               becomes an integration test and is also run.
  ```rust,ignore               not compiled. The nearest non-blank line above the fence must
                               contain "Not compiled:" followed by the reason.
  ```rust,compile_fail         must fail to compile. ```rust,compile_fail,E0277 must fail with
                               E0277 and no other error.
Any other info string that starts with "rust" (in any case), and any ~~~ fence for Rust, is an
error, so no block is skipped silently. The only allowed #[ignore] is
#[ignore = "needs SurrealDB"]; those tests run in --live mode.

Shared definitions: a definition that several blocks need (StoreError, ApiError, ...) has one
canonical region and exact copies, each between a marker line and `// end: <Name>`:
  // canonical: rust-errors/mapping.md StoreError        (in that file)
  // copy of: rust-errors/mapping.md StoreError          (anywhere else)
A type, enum, fn or trait named like a region may not be defined outside such a region.
--fix-copies rewrites every copy from its canonical region.

Every compiled file starts with `#![allow(dead_code, reason = ...)]`, because examples define
items they never call. Nothing else is allowed.

Usage:
  python3 .claude/skills/rust/scripts/check_examples.py [--skill NAME ...] [--no-test] [--live]
                                                        [--keep] [--fix-copies]

The build cache lives in $PDM_RUST_SKILLS_TARGET_DIR, or ~/.cache/pdm-rust-skills-check/target.
Python 3.11+ standard library only; --live also needs docker and cargo-nextest.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

SCRIPT = Path(__file__).resolve()
SKILLS_DIR = SCRIPT.parents[2]          # .claude/skills
REPO = SCRIPT.parents[4]                # repository root
VERSIONS_MD = SKILLS_DIR / "rust" / "versions.md"
LINTS_MD = SKILLS_DIR / "rust-cargo-lints" / "SKILL.md"
DATABASE_MD = REPO / "docs" / "database" / "README.md"
SCHEMA = REPO / "docs" / "database" / "schema.surql"

HEADER = (
    '#![allow(dead_code, reason = "skill examples define items they do not call")]\n'
)
FENCE_RE = re.compile(r"^(?P<indent>[ \t]*)(?P<ticks>`{3,}|~{3,})(?P<info>[^`]*)$")
TEST_RE = re.compile(r"#\[(?:tokio::)?test\b|#\[cfg\(test\)\]|\bproptest!\s*\{")
MAIN_RE = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+main\s*\(", re.M)
REASON_RE = re.compile(r"Not compiled:\s*\S")
ERROR_CODE_RE = re.compile(r"^E\d{4}$")
SUMMARY_RE = re.compile(r"^(?:\d+ warnings? emitted|aborting due to|\d+ warnings?.*errors? emitted)")
IGNORE_ATTR_RE = re.compile(r"#\[ignore\b(?P<rest>[^\]]*)\]")
LIVE_IGNORE = '#[ignore = "needs SurrealDB"]'
REGION_RE = re.compile(r"^\s*// (?P<kind>canonical|copy of): (?P<file>[\w./-]+\.md) (?P<name>\w+)\s*$")
END_RE = re.compile(r"^\s*// end: (?P<name>\w+)\s*$")
LIBTEST_LINE_RE = re.compile(r"^test (?P<name>\S+)(?: - should panic)? \.\.\. (?P<result>ok|FAILED|ignored(?:, (?P<reason>.*))?)$")
IMAGE_RE = re.compile(r"surrealdb/surrealdb:v[\d.]+@sha256:[0-9a-f]{64}")


@dataclass
class Region:
    kind: str                 # "canonical" or "copy of"
    file: str                 # the canonical file named in the marker
    name: str
    block: "Block"
    start: int                # index in block.code lines of the marker
    end: int                  # index of the end marker
    lines: list[str]


@dataclass
class Block:
    md: Path
    line: int                 # 1-based line of the opening fence
    kind: str                 # "check", "ignore" or "compile_fail"
    code: str
    indent: str = ""
    error_code: str | None = None
    name: str = ""
    target: str = ""          # "example" or "test"
    header_lines: int = 0
    regions: list[Region] = field(default_factory=list)

    @property
    def where(self) -> str:
        return f"{self.md.relative_to(REPO)}:{self.line}"

    @property
    def skill_path(self) -> str:
        return str(self.md.relative_to(SKILLS_DIR))


def fail(msg: str) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(2)


# ---------------------------------------------------------------- markdown parsing

def extract_blocks(md: Path) -> tuple[list[Block], list[str]]:
    blocks: list[Block] = []
    errors: list[str] = []
    lines = md.read_text(encoding="utf-8").splitlines()
    rel = md.relative_to(REPO)
    i = 0
    while i < len(lines):
        m = FENCE_RE.match(lines[i])
        if not m:
            i += 1
            continue
        indent, ticks, info = m.group("indent"), m.group("ticks"), m.group("info").strip()
        start = i
        body: list[str] = []
        i += 1
        while i < len(lines):
            close = lines[i].strip()
            if close.startswith(ticks) and close.strip(ticks[0]) == "":
                break
            line = lines[i]
            body.append(line[len(indent):] if line.startswith(indent) else line.lstrip())
            i += 1
        else:
            errors.append(f"{rel}:{start + 1}: unclosed code fence")
        i += 1
        words = [w.strip() for w in info.split(",")] if info else []
        lang = words[0] if words else ""
        is_rustish = lang.lower().startswith("rust") or lang.lower() in {"rs"}
        if ticks[0] == "~":
            if is_rustish:
                errors.append(f"{rel}:{start + 1}: use a ``` fence for Rust, not {ticks}{info}")
            continue
        if not is_rustish:
            continue
        if lang != "rust" or any(" " in w or "=" in w for w in words):
            errors.append(f"{rel}:{start + 1}: unknown fence info ```{info}; use rust, rust,ignore "
                          "or rust,compile_fail[,E0000]")
            continue
        attrs = words[1:]
        code = "\n".join(body) + "\n"
        block = Block(md=md, line=start + 1, kind="check", code=code, indent=indent)
        unknown = [a for a in attrs if a not in {"ignore", "compile_fail"} and not ERROR_CODE_RE.match(a)]
        if unknown:
            errors.append(f"{block.where}: unknown fence attribute(s) {unknown}; use rust, rust,ignore or rust,compile_fail[,E0000]")
            continue
        if "ignore" in attrs and "compile_fail" in attrs:
            errors.append(f"{block.where}: a block can't be both ignore and compile_fail")
            continue
        if "ignore" in attrs:
            block.kind = "ignore"
            above = next((lines[j] for j in range(start - 1, -1, -1) if lines[j].strip()), "")
            if not REASON_RE.search(above):
                errors.append(f"{block.where}: ```rust,ignore needs a line above it with 'Not compiled: <reason>'")
        elif "compile_fail" in attrs:
            block.kind = "compile_fail"
            codes = [a for a in attrs if ERROR_CODE_RE.match(a)]
            if len(codes) > 1:
                errors.append(f"{block.where}: name at most one error code")
                continue
            block.error_code = codes[0] if codes else None
        elif any(ERROR_CODE_RE.match(a) for a in attrs):
            errors.append(f"{block.where}: an error code only goes with compile_fail")
            continue
        blocks.append(block)
    return blocks, errors


def find_regions(b: Block) -> list[str]:
    """Fill b.regions from its markers; return marker problems."""
    errors = []
    lines = b.code.splitlines()
    open_region: tuple[str, str, str, int] | None = None
    for idx, line in enumerate(lines):
        m = REGION_RE.match(line)
        e = END_RE.match(line)
        if m:
            if open_region:
                errors.append(f"{b.where}: region {open_region[2]} is not closed before the next marker")
            open_region = (m.group("kind"), m.group("file"), m.group("name"), idx)
        elif e:
            if not open_region or open_region[2] != e.group("name"):
                errors.append(f"{b.where}: `// end: {e.group('name')}` without its opening marker")
                continue
            kind, file, name, start = open_region
            b.regions.append(Region(kind, file, name, b, start, idx, lines[start + 1:idx]))
            open_region = None
        elif re.match(r"^\s*// (canonical|copy of)\b", line):
            errors.append(f"{b.where}: malformed region marker {line.strip()!r}; "
                          "write `// canonical: <skill>/<file>.md <Name>`")
    if open_region:
        errors.append(f"{b.where}: region {open_region[2]} has no `// end: {open_region[2]}`")
    return errors


def check_regions(blocks: list[Block], all_blocks: list[Block]) -> tuple[list[str], list[Region]]:
    """Copies must equal their canonical region. Returns problems and the copies that differ."""
    problems: list[str] = []
    stale: list[Region] = []
    canonical: dict[str, Region] = {}
    for b in all_blocks:
        for r in b.regions:
            if r.kind != "canonical":
                continue
            if r.file != b.skill_path:
                problems.append(f"{b.where}: canonical region {r.name} names {r.file}, but it is in {b.skill_path}")
            if r.name in canonical:
                problems.append(f"{b.where}: second canonical region {r.name} (first at {canonical[r.name].block.where})")
            canonical[r.name] = r
            if b.kind != "check":
                problems.append(f"{b.where}: canonical region {r.name} must be in a compiled ```rust block")
    for b in blocks:
        for r in b.regions:
            if r.kind == "canonical":
                continue
            c = canonical.get(r.name)
            if c is None:
                problems.append(f"{b.where}: copy of {r.name} has no canonical region")
                continue
            if c.block.skill_path != r.file:
                problems.append(f"{b.where}: copy of {r.name} names {r.file}; the canonical is in {c.block.skill_path}")
            if r.lines != c.lines:
                diff = "\n".join(difflib.unified_diff(c.lines, r.lines, f"canonical {c.block.where}",
                                                      f"copy {b.where}", lineterm="", n=1))
                problems.append(f"[copies] {b.where}: copy of {r.name} differs from {c.block.where} "
                                f"(run with --fix-copies):\n{diff[:3000]}")
                stale.append(r)
    # A definition with a region's name must be inside a region of that name.
    for b in blocks:
        if b.kind == "ignore":
            continue
        lines = b.code.splitlines()
        inside = {(r.name, i) for r in b.regions for i in range(r.start, r.end + 1)}
        for name in canonical:
            pat = re.compile(rf"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:struct|enum|fn|trait|type)\s+{name}\b")
            for i, line in enumerate(lines):
                if pat.match(line) and (name, i) not in inside:
                    problems.append(f"{b.md.relative_to(REPO)}:{b.line + 1 + i}: {name} is defined outside a "
                                    f"`// copy of: {canonical[name].block.skill_path} {name}` region")
    return problems, stale


def fix_copies(stale: list[Region], canonical_by_name: dict[str, Region]) -> None:
    by_md: dict[Path, list[Region]] = {}
    for r in stale:
        by_md.setdefault(r.block.md, []).append(r)
    for md, regions in by_md.items():
        lines = md.read_text(encoding="utf-8").splitlines()
        # Bottom-up, so earlier line numbers stay valid.
        for r in sorted(regions, key=lambda r: (r.block.line, r.start), reverse=True):
            first = r.block.line + r.start + 1          # 0-based md index of the line after the marker
            last = r.block.line + r.end                 # 0-based md index of the end marker
            new = [(r.block.indent + l) if l else "" for l in canonical_by_name[r.name].lines]
            lines[first:last] = new
        md.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"rewrote {len(regions)} copy region(s) in {md.relative_to(REPO)}")


def check_ignores(b: Block) -> list[str]:
    problems = []
    for i, line in enumerate(b.code.splitlines()):
        for m in IGNORE_ATTR_RE.finditer(line):
            if m.group(0) != LIVE_IGNORE:
                problems.append(f"{b.md.relative_to(REPO)}:{b.line + 1 + i}: the only allowed #[ignore] is "
                                f"{LIVE_IGNORE}, which --live runs; got {m.group(0)}")
    return problems


def md_table_rows(text: str, heading: str) -> list[list[str]]:
    """Rows of the first table after a '### heading' line, without the header rows."""
    lines = text.splitlines()
    try:
        start = next(i for i, l in enumerate(lines) if l.strip() == f"### {heading}")
    except StopIteration:
        fail(f"{VERSIONS_MD.relative_to(REPO)} has no '### {heading}' section")
    rows: list[list[str]] = []
    seen_table = False
    for line in lines[start + 1:]:
        if line.startswith("#"):
            break
        if line.startswith("|"):
            seen_table = True
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            rows.append(cells)
        elif seen_table:
            break
    return [r for r in rows[2:]]  # drop the header and the |---| row


def dependency_toml(rows: list[list[str]]) -> str:
    out = []
    for row in rows:
        if len(row) < 3:
            fail(f"bad crate row in versions.md: {row}")
        name = row[0].strip("`")
        version = row[1].strip("`")
        if not re.match(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?$", version):
            fail(f"crate {name}: version {version!r} is not an exact version")
        features = re.findall(r"`([^`]+)`", row[2])
        default_off = "default features off" in row[2]
        spec = f'version = "={version}"'
        if default_off:
            spec += ", default-features = false"
        if features:
            spec += ", features = [" + ", ".join(f'"{f}"' for f in features) + "]"
        out.append(f'{name} = {{ {spec} }}')
    return "\n".join(out) + "\n"


def toolchain_version(text: str) -> str:
    m = re.search(r"^\| Rust \| (\d+\.\d+) \|", text, re.M)
    if not m:
        fail("versions.md: no '| Rust | x.y |' row in the toolchain table")
    return m.group(1)


def nextest_pin(text: str) -> str:
    m = re.search(r"^\| cargo-nextest \| (\d+\.\d+\.\d+) \|", text, re.M)
    if not m:
        fail("versions.md: no '| cargo-nextest | x.y.z |' row in the toolchain table")
    return m.group(1)


def lint_config() -> tuple[str, str]:
    """The [lints] table and clippy.toml from the rust-cargo-lints skill."""
    if not LINTS_MD.exists():
        fail(f"{LINTS_MD.relative_to(REPO)} is missing; it holds the lint settings the check uses")
    text = LINTS_MD.read_text(encoding="utf-8")
    tomls = re.findall(r"^```toml\n(.*?)^```", text, re.M | re.S)
    cargo = next((t for t in tomls if t.startswith("# Cargo.toml (workspace root")), None)
    clippy = next((t for t in tomls if t.startswith("# clippy.toml")), None)
    if cargo is None or clippy is None:
        fail("rust-cargo-lints/SKILL.md needs a ```toml block starting '# Cargo.toml (workspace root' "
             "and one starting '# clippy.toml'")
    parsed = tomllib.loads(cargo)
    lints = parsed.get("workspace", {}).get("lints")
    if not lints:
        fail("the '# Cargo.toml (workspace root)' block has no [workspace.lints] table")
    tomllib.loads(clippy)  # must parse
    out = []
    for tool, table in lints.items():
        out.append(f"[lints.{tool}]")
        for lint, level in table.items():
            if isinstance(level, dict):
                inner = ", ".join(f'{k} = {json.dumps(v)}' for k, v in level.items())
                out.append(f"{lint} = {{ {inner} }}")
            else:
                out.append(f"{lint} = {json.dumps(level)}")
        out.append("")
    return "\n".join(out), clippy


# ---------------------------------------------------------------- crate generation

def sanitize(s: str) -> str:
    return re.sub(r"[^a-z0-9_]", "_", s.lower())


def prepare(blocks: list[Block]) -> None:
    for b in blocks:
        skill = b.md.relative_to(SKILLS_DIR).parts[0]
        stem = b.md.stem
        b.name = sanitize(f"{skill}__{stem}__l{b.line}")
        b.target = "test" if TEST_RE.search(b.code) else "example"
        b.header_lines = HEADER.count("\n")


def file_text(b: Block) -> str:
    text = HEADER + b.code
    if b.target == "example" and not MAIN_RE.search(b.code):
        text += "\nfn main() {}\n"
    return text


def write_crate(root: Path, blocks: list[Block], deps: str, dev_deps: str, lints: str, clippy: str) -> None:
    """The crate sits at <tmp>/backend/crates/skill-examples, as a store crate would in the
    repository, so `include_str!("../../../migrations/schema.surql")` finds the schema."""
    (root / "src").mkdir(parents=True)
    (root / "examples").mkdir()
    (root / "tests").mkdir()
    migrations = root.parents[1] / "migrations"
    migrations.mkdir(parents=True)
    shutil.copyfile(SCHEMA, migrations / "schema.surql")
    (root / "src" / "lib.rs").write_text("//! Empty: the examples and tests are the point.\n")
    (root / "rustfmt.toml").write_text("# empty: default options only\n")
    targets = []
    for b in blocks:
        folder = "tests" if b.target == "test" else "examples"
        (root / folder / f"{b.name}.rs").write_text(file_text(b))
        if b.target == "test":
            targets.append(f'[[test]]\nname = "{b.name}"\npath = "tests/{b.name}.rs"\n')
        else:
            targets.append(f'[[example]]\nname = "{b.name}"\npath = "examples/{b.name}.rs"\n')
    manifest = (
        "[package]\nname = \"pdm-rust-skill-examples\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"
        "publish = false\nautoexamples = false\nautotests = false\n\n"
        f"[dependencies]\n{deps}\n[dev-dependencies]\n{dev_deps}\n{lints}\n"
        + "\n".join(targets)
    )
    (root / "Cargo.toml").write_text(manifest)
    (root / "clippy.toml").write_text(clippy)


# ---------------------------------------------------------------- running cargo

def cargo_env(target_dir: Path) -> dict[str, str]:
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir), CARGO_TERM_COLOR="never")
    for var in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS"):
        env.pop(var, None)
    return env


def run_cargo(root: Path, target_dir: Path, args: list[str]) -> tuple[int, list[dict], str]:
    proc = subprocess.run(
        ["cargo", *args, "--message-format=json"],
        cwd=root, env=cargo_env(target_dir), capture_output=True, text=True,
    )
    messages = []
    for line in proc.stdout.splitlines():
        if line.startswith("{"):
            try:
                messages.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return proc.returncode, messages, proc.stderr


def map_line(b: Block, line_in_file: int) -> int:
    return b.line + (line_in_file - b.header_lines)


def diagnostics(messages: list[dict], by_name: dict[str, Block]) -> dict[str, list[dict]]:
    found: dict[str, list[dict]] = {}
    for m in messages:
        if m.get("reason") != "compiler-message":
            continue
        name = m.get("target", {}).get("name", "")
        if name not in by_name:
            continue
        msg = m["message"]
        if not msg.get("spans") and SUMMARY_RE.match(msg.get("message", "")):
            continue
        found.setdefault(name, []).append(msg)
    return found


def describe(b: Block, msg: dict) -> str:
    span = next((s for s in msg.get("spans", []) if s.get("is_primary")), None)
    where = b.where
    if span and span.get("file_name", "").endswith(f"{b.name}.rs"):
        where = f"{b.md.relative_to(REPO)}:{map_line(b, span['line_start'])}"
    code = (msg.get("code") or {}).get("code") or ""
    return f"{where}: {msg['level']}{f'[{code}]' if code else ''}: {msg['message']}"


def target_args(blocks: list[Block]) -> list[str]:
    args: list[str] = []
    for b in blocks:
        args += ["--test" if b.target == "test" else "--example", b.name]
    return args


def evaluate(stage: str, messages: list[dict], blocks: list[Block], stderr: str, rc: int) -> list[str]:
    """Problems in blocks that must compile cleanly: any warning or error fails."""
    by_name = {b.name: b for b in blocks}
    failures = []
    for name, ds in diagnostics(messages, by_name).items():
        for d in ds:
            failures.append(f"[{stage}] {describe(by_name[name], d)}")
    if rc != 0 and not failures:
        failures.append(f"[{stage}] cargo exited with {rc}:\n{stderr[-3000:]}")
    return failures


def evaluate_compile_fail(messages: list[dict], blocks: list[Block]) -> list[str]:
    by_name = {b.name: b for b in blocks}
    diags = diagnostics(messages, by_name)
    failures = []
    for b in blocks:
        errors = [d for d in diags.get(b.name, []) if d["level"] == "error"]
        codes = sorted({(d.get("code") or {}).get("code") or "no code" for d in errors})
        if not errors:
            failures.append(f"[compile_fail] {b.where}: the block compiled without errors")
        elif b.error_code and codes != [b.error_code]:
            detail = "; ".join(describe(b, d) for d in errors)[:1500]
            failures.append(f"[compile_fail] {b.where}: expected only {b.error_code}, got {codes}: {detail}")
    return failures


def run_rustfmt(root: Path, blocks: list[Block]) -> list[str]:
    failures = []
    config = root / "rustfmt.toml"
    for b in blocks:
        folder = "tests" if b.target == "test" else "examples"
        path = root / folder / f"{b.name}.rs"
        proc = subprocess.run(
            ["rustfmt", "--check", "--color", "never", "--edition", "2024",
             "--config-path", str(config), str(path)],
            cwd=root, capture_output=True, text=True,
        )
        if proc.returncode != 0:
            m = re.search(r":(\d+):", proc.stdout)
            line = map_line(b, int(m.group(1))) if m else b.line
            failures.append(f"[rustfmt] {b.md.relative_to(REPO)}:{line}: not rustfmt-clean "
                            f"(run rustfmt on the block)\n{(proc.stdout + proc.stderr).strip()[:1500]}")
    return failures


@dataclass
class TestCounts:
    run: int = 0
    passed: int = 0
    ignored: list[tuple[str, str, str]] = field(default_factory=list)   # (block, test, reason)


def run_tests(root: Path, target_dir: Path, tests: list[Block], counts: TestCounts) -> list[str]:
    if not tests:
        return []
    env = cargo_env(target_dir)
    env["INSTA_UPDATE"] = "no"   # a snapshot mismatch fails instead of writing a .snap.new
    env["CI"] = "true"
    build = subprocess.run(["cargo", "test", "--no-run", *target_args(tests)],
                           cwd=root, env=env, capture_output=True, text=True)
    if build.returncode != 0:
        return [f"[test] building the test blocks failed:\n{build.stderr[-4000:]}"]
    failures = []
    for b in tests:
        proc = subprocess.run(["cargo", "test", "--test", b.name],
                              cwd=root, env=env, capture_output=True, text=True)
        seen = 0
        for line in proc.stdout.splitlines():
            m = LIBTEST_LINE_RE.match(line.strip())
            if not m:
                continue
            seen += 1
            result = m.group("result")
            if result == "ok":
                counts.run += 1
                counts.passed += 1
            elif result == "FAILED":
                counts.run += 1
            else:
                counts.ignored.append((b.where, m.group("name"), m.group("reason") or "(no reason)"))
        if proc.returncode != 0:
            failures.append(f"[test] {b.where}: test failed\n{(proc.stdout + proc.stderr)[-2500:]}")
        elif seen == 0:
            failures.append(f"[test] {b.where}: the block looks like a test but no test ran")
    return failures


# ---------------------------------------------------------------- live SurrealDB tests

def pinned_image() -> str:
    text = DATABASE_MD.read_text(encoding="utf-8")
    m = IMAGE_RE.search(text)
    if not m:
        fail(f"{DATABASE_MD.relative_to(REPO)} has no surrealdb/surrealdb:vX@sha256:... pin")
    return m.group(0)


def start_surrealdb(image: str) -> tuple[str, int]:
    name = f"pdm-skills-surrealdb-{os.getpid()}"
    proc = subprocess.run(
        ["docker", "run", "-d", "--rm", "--name", name, "-p", "127.0.0.1::8000", image,
         "start", "--user", "root", "--pass", "root", "memory"],
        capture_output=True, text=True,
    )
    if proc.returncode != 0:
        fail(f"docker run {image} failed: {proc.stderr.strip()}")
    port_out = subprocess.run(["docker", "port", name, "8000/tcp"], capture_output=True, text=True).stdout
    m = re.search(r":(\d+)\s*$", port_out.strip().splitlines()[0] if port_out.strip() else "")
    if not m:
        stop_surrealdb(name)
        fail(f"could not read the published port of {name}: {port_out!r}")
    port = int(m.group(1))
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                if r.status == 200:
                    return name, port
        except (urllib.error.URLError, ConnectionError, OSError):
            pass
        time.sleep(0.5)
    stop_surrealdb(name)
    fail(f"{image} did not become healthy within 60 s")
    raise AssertionError  # unreachable


def stop_surrealdb(name: str) -> None:
    subprocess.run(["docker", "stop", name], capture_output=True, text=True)


def run_live(root: Path, target_dir: Path, live: list[Block], versions: str) -> tuple[list[str], str]:
    want = nextest_pin(versions)
    got = subprocess.run(["cargo", "nextest", "--version"], capture_output=True, text=True).stdout
    m = re.search(r"cargo-nextest (\d+\.\d+\.\d+)", got)
    if not m:
        return [f"[live] cargo-nextest is not installed (cargo install cargo-nextest --version {want} --locked)"], ""
    if m.group(1) != want:
        return [f"[live] versions.md pins cargo-nextest {want}, but {m.group(1)} is installed"], ""
    image = pinned_image()
    name, port = start_surrealdb(image)
    try:
        env = cargo_env(target_dir)
        env["PDM_TEST_SURREALDB_URL"] = f"ws://127.0.0.1:{port}"
        env["CI"] = "true"
        proc = subprocess.run(
            ["cargo", "nextest", "run", "--run-ignored", "only", "--no-fail-fast",
             "--failure-output", "final", *target_args(live)],
            cwd=root, env=env, capture_output=True, text=True,
        )
        out = proc.stdout + proc.stderr
        summary = next((l.strip() for l in out.splitlines() if re.search(r"Summary \[", l)), "")
        failures = []
        if proc.returncode != 0:
            failures.append(f"[live] cargo nextest failed against {image}:\n{out[-5000:]}")
        elif not re.search(r"\b[1-9]\d* tests? run: [1-9]\d* passed", summary):
            failures.append(f"[live] no SurrealDB test ran:\n{out[-2000:]}")
        return failures, f"{summary} (image {image}, nextest {want})"
    finally:
        stop_surrealdb(name)


# ---------------------------------------------------------------- main

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--skill", action="append", help="only check this skill directory (repeatable), e.g. rust-async")
    ap.add_argument("--no-test", action="store_true", help="skip running the test blocks")
    ap.add_argument("--live", action="store_true",
                    help="also run the #[ignore = \"needs SurrealDB\"] tests against the pinned image")
    ap.add_argument("--keep", action="store_true", help="keep the temporary crate and print its path")
    ap.add_argument("--fix-copies", action="store_true", help="rewrite every copy region from its canonical region")
    args = ap.parse_args()

    target_dir = Path(os.environ.get("PDM_RUST_SKILLS_TARGET_DIR",
                                     Path.home() / ".cache" / "pdm-rust-skills-check" / "target")).resolve()
    if REPO in target_dir.parents or target_dir == REPO:
        fail(f"the target directory {target_dir} is inside the repository; use a path outside it")

    versions = VERSIONS_MD.read_text(encoding="utf-8")
    want = toolchain_version(versions)
    rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
    m = re.match(r"rustc (\d+\.\d+)\.", rustc)
    if not m or m.group(1) != want:
        fail(f"versions.md pins Rust {want}, but `rustc --version` is {rustc!r}")
    deps = dependency_toml(md_table_rows(versions, "Dependencies"))
    dev_deps = dependency_toml(md_table_rows(versions, "Dev-dependencies"))
    lints, clippy = lint_config()

    all_mds = sorted(p for d in SKILLS_DIR.glob("rust*") if d.is_dir() for p in d.rglob("*.md"))
    known = {p.relative_to(SKILLS_DIR).parts[0] for p in all_mds}
    if args.skill:
        missing = sorted(set(args.skill) - known)
        if missing:
            fail(f"no skill directory {missing}; known: {sorted(known)}")
    all_blocks: list[Block] = []
    parse_errors: list[str] = []
    for md in all_mds:
        bs, errs = extract_blocks(md)
        all_blocks += bs
        parse_errors += errs
    for b in all_blocks:
        parse_errors += find_regions(b)
    selected = set(args.skill) if args.skill else known
    blocks = [b for b in all_blocks if b.md.relative_to(SKILLS_DIR).parts[0] in selected]
    parse_errors = [e for e in parse_errors
                    if Path(e.split(":", 1)[0]).parts[2:3] and Path(e.split(":", 1)[0]).parts[2] in selected]
    mds = [p for p in all_mds if p.relative_to(SKILLS_DIR).parts[0] in selected]
    if not blocks:
        fail(f"no ```rust blocks found in {sorted(selected)}")

    region_problems, stale = check_regions(blocks, all_blocks)
    if args.fix_copies:
        canon = {r.name: r for b in all_blocks for r in b.regions if r.kind == "canonical"}
        fix_copies(stale, canon)
        print("re-run without --fix-copies to check")
        return 0

    compiled = [b for b in blocks if b.kind != "ignore"]
    ignored = [b for b in blocks if b.kind == "ignore"]
    prepare(compiled)
    for b in compiled:
        parse_errors += check_ignores(b)
    print(f"{rustc}; {len(mds)} files, {len(blocks)} rust blocks: "
          f"{sum(b.kind == 'check' for b in blocks)} compiled, "
          f"{sum(b.kind == 'compile_fail' for b in blocks)} compile_fail, {len(ignored)} not compiled; "
          f"{sum(len(b.regions) for b in blocks)} shared regions")

    failures = list(parse_errors) + region_problems
    tmp = Path(tempfile.mkdtemp(prefix="pdm-rust-skill-examples-"))
    if REPO in tmp.parents:
        fail(f"temporary directory {tmp} is inside the repository")
    crate = tmp / "backend" / "crates" / "skill-examples"
    counts = TestCounts()
    live_summary = ""
    try:
        write_crate(crate, compiled, deps, dev_deps, lints, clippy)
        checked = [b for b in compiled if b.kind == "check"]
        must_fail = [b for b in compiled if b.kind == "compile_fail"]
        fmt_failures = run_rustfmt(crate, compiled)
        failures += fmt_failures
        check_failures: list[str] = []
        clippy_failures: list[str] = []
        if checked:
            rc, msgs, err = run_cargo(crate, target_dir, ["check", "--keep-going", *target_args(checked)])
            check_failures = evaluate("check", msgs, checked, err, rc)
            rc, msgs, err = run_cargo(crate, target_dir, ["clippy", "--keep-going", *target_args(checked)])
            clippy_failures = [f for f in evaluate("clippy", msgs, checked, err, rc)
                               if f.replace("[clippy]", "[check]", 1) not in check_failures]
        failures += check_failures + clippy_failures
        cf_failures: list[str] = []
        if must_fail:
            _, msgs, _ = run_cargo(crate, target_dir, ["check", "--keep-going", *target_args(must_fail)])
            cf_failures = evaluate_compile_fail(msgs, must_fail)
            failures += cf_failures
        test_failures: list[str] = []
        tests = [b for b in checked if b.target == "test"]
        if not args.no_test and not check_failures:
            test_failures = run_tests(crate, target_dir, tests, counts)
            failures += test_failures
        live_failures: list[str] = []
        live_blocks = [b for b in tests if LIVE_IGNORE in b.code]
        if args.live and not check_failures:
            if live_blocks:
                live_failures, live_summary = run_live(crate, target_dir, live_blocks, versions)
            else:
                live_failures = ["[live] no block has a #[ignore = \"needs SurrealDB\"] test"]
            failures += live_failures
        print()
        print("Summary")
        print(f"  blocks found:        {len(blocks)}")
        print(f"  compiled:            {len(checked)} ({sum(b.target == 'test' for b in checked)} as tests, "
              f"{sum(b.target == 'example' for b in checked)} as examples)")
        print(f"  compile_fail:        {len(must_fail)} ({'ok' if not cf_failures else f'{len(cf_failures)} problem(s)'})")
        print(f"  not compiled:        {len(ignored)}")
        for b in ignored:
            lines = b.md.read_text(encoding="utf-8").splitlines()
            above = next((lines[j] for j in range(b.line - 2, -1, -1) if lines[j].strip()), "")
            reason = above[above.find("Not compiled:"):].strip() if "Not compiled:" in above else "(no reason)"
            print(f"    {b.where}: {reason}")
        print(f"  shared regions:      {sum(len(b.regions) for b in blocks)} "
              f"({'ok' if not region_problems else f'{len(region_problems)} problem(s)'})")
        print(f"  rustfmt:             {'ok' if not fmt_failures else f'{len(fmt_failures)} problem(s)'}")
        print(f"  check:               {'ok' if not check_failures else f'{len(check_failures)} problem(s)'}")
        print(f"  clippy:              {'ok' if not clippy_failures else f'{len(clippy_failures)} problem(s)'}")
        if args.no_test:
            print("  tests:               skipped (--no-test)")
        elif check_failures:
            print("  tests:               not run (check failed)")
        else:
            print(f"  test blocks:         {len(tests)} ({'ok' if not test_failures else f'{len(test_failures)} problem(s)'})")
            print(f"  tests run:           {counts.run}, passed {counts.passed}, ignored {len(counts.ignored)}")
            for where, name, reason in counts.ignored:
                print(f"    ignored {where} {name}: {reason}"
                      + ("" if args.live else " (run with --live)"))
        if args.live:
            print(f"  live:                {live_summary or 'not run'} "
                  f"({'ok' if not live_failures else f'{len(live_failures)} problem(s)'})")
        if failures:
            print()
            print(f"FAILED: {len(failures)} problem(s)")
            for f in failures:
                print(f"  {f}")
            return 1
        print()
        print("OK: every block passed" + (", live tests included" if args.live else ""))
        return 0
    finally:
        if args.keep:
            print(f"kept {crate}")
        else:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())

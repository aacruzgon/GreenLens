#!/usr/bin/env python3
"""Generate one Claude Code skill per page and per group of Apple's Human Interface Guidelines.

Usage (from the repository root):
  python3 .claude/skills/hig/scripts/build_skills.py

It walks the guidelines' navigation tree live from developer.apple.com, then
rewrites every .claude/skills/hig-*/SKILL.md and .claude/skills/hig/index.md.
The hand-written hig skill (SKILL.md, expo.md, scripts/) is left alone.

A skill holds the page's links, never its text: the page link, a link to every
section, the related guideline pages (with their skills), developer
documentation and videos. The skill tells Claude to read the page live with
hig_page.py. Rerun this script when Apple adds, renames or moves a page.
Standard library only.
"""
import json
import os
import re
import shutil
import sys
import time

sys.dont_write_bytecode = True  # no __pycache__ inside the skill
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hig_page import HIG, SITE, absolute, fetch  # noqa: E402

SKILLS = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
ROOT_TITLE = "Human Interface Guidelines"
SKIP_HEADINGS = {"Resources", "Change log"}
PLATFORM_NAMES = {"ios": "iOS", "ipados": "iPadOS", "macos": "macOS", "tvos": "tvOS",
                  "visionos": "visionOS", "watchos": "watchOS"}
PHONE = {"ios", "ipados"}
MAX_NAME = 64


def page_url(slug):
    return f"{SITE}{HIG}/{slug}" if slug else SITE + HIG


def crawl():
    """Every node of the tree, parents before children, with its skill name."""
    nodes, seen = [], set()

    def walk(slug, chain):
        if slug in seen:
            return
        seen.add(slug)
        page = fetch(slug)
        time.sleep(0.1)
        meta = page.get("metadata", {})
        node = {
            "slug": slug,
            "title": meta.get("title", slug),
            "role": meta.get("role"),
            "chain": chain,  # ancestor nodes, nearest last, root excluded
            "page": page,
            "children": [],
        }
        node["name"] = "hig" if not slug else "hig-" + "-".join([c["slug"] for c in chain] + [slug])
        nodes.append(node)
        for section in page.get("topicSections", []):
            for ident in section.get("identifiers", []):
                url = page["references"].get(ident, {}).get("url", "")
                if url.startswith(HIG + "/"):
                    child = url[len(HIG) + 1:]
                    node["children"].append(child)
                    walk(child, chain + ([node] if slug else []))

    walk("", [])
    return nodes


def content_of(page):
    return [b for s in page.get("primaryContentSections", []) for b in s.get("content", [])]


def sections(page):
    """(level, title, anchor) for the h2 and h3 headings a reader would jump to."""
    out, skipping = [], False
    for b in content_of(page):
        if b.get("type") != "heading":
            continue
        level = b.get("level", 2)
        if level == 2:
            skipping = b.get("text") in SKIP_HEADINGS
        if not skipping and level in (2, 3) and b.get("anchor"):
            out.append((level, b["text"], b["anchor"]))
    return out


def resources(page):
    """Links under Resources: {"Related": [...], "Developer documentation": [...], "Videos": [...]}."""
    refs = page.get("references", {})
    groups, current, in_resources = {}, None, False

    def add(ident):
        r = refs.get(ident, {})
        if r.get("url") and current:
            url = absolute(r["url"])
            item = (r.get("title") or ident.rsplit("/", 1)[-1], url)
            if item not in groups[current]:
                groups[current].append(item)

    def scan(x):
        if isinstance(x, dict):
            if x.get("type") == "reference":
                add(x.get("identifier", ""))
            for v in x.values():
                scan(v)
        elif isinstance(x, list):
            for v in x:
                scan(v)

    for b in content_of(page):
        if b.get("type") == "heading":
            if b.get("level") == 2:
                in_resources = b.get("text") == "Resources"
                current = None
            elif in_resources and b.get("level", 2) >= 3:
                current = b.get("text")
                groups.setdefault(current, [])
            continue
        if not current:
            continue
        if b.get("type") == "links":
            for ident in b.get("items", []):
                add(ident)
        else:
            scan(b)
    return groups


def last_changed(page):
    """The newest date in the page's change log table, if it has one."""
    found, dates = False, []
    for b in content_of(page):
        if b.get("type") == "heading" and b.get("level") == 2:
            found = b.get("text") == "Change log"
        elif found and b.get("type") == "table":
            for m in re.finditer(r'"text": "([A-Z][a-z]+ \d{1,2}, \d{4})"', json.dumps(b.get("rows", []))):
                dates.append(time.strptime(m.group(1), "%B %d, %Y"))
    return time.strftime("%B %-d, %Y", max(dates)) if dates else None


def platforms(page):
    raw = page.get("metadata", {}).get("customMetadata", {}).get("supported-platforms")
    return [p for p in raw.split(",") if p] if raw else []


def yaml_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def breadcrumb(node):
    parts = [f"[{ROOT_TITLE}]({page_url('')})"]
    parts += [f"[{c['title']}]({page_url(c['slug'])})" for c in node["chain"]]
    return " › ".join(parts + [node["title"]])


def platform_line(plats):
    if not plats:
        return "**Platforms:** all (the page names none)."
    names = ", ".join(PLATFORM_NAMES.get(p, p) for p in plats)
    if PHONE & set(plats):
        return f"**Platforms:** {names}."
    return (f"**Platforms:** {names} only. The PDM app ships on iPhone, Android and the web, so this "
            "page applies only where it says something general; say so before applying it. "
            "Claude doesn't load this skill on its own; use it by name.")


def phone_relevant(node):
    plats = platforms(node["page"])
    return not plats or bool(PHONE & set(plats))


def description(node, by_slug):
    """Short, so 172 skills fit Claude Code's skill listing budget."""
    path = " › ".join(["Apple HIG"] + [c["title"] for c in node["chain"]] + [node["title"]])
    if node["role"] == "article":
        topics = [t for lvl, t, _ in sections(node["page"]) if lvl == 2 and t != "Platform considerations"]
        covers = f": {', '.join(topics[:6])}" if topics else ""
        text = f"{path}{covers}. Use for design, build or review work on {node['title']}."
        if not phone_relevant(node):
            names = ", ".join(PLATFORM_NAMES.get(p, p) for p in platforms(node["page"]))
            text += f" {names} only."
    else:
        kids = ", ".join(by_slug[c]["title"] for c in node["children"])
        text = f"{path} (group): {kids}. Use to pick which of these pages a task needs."
    return text[:1000]


HOW_TO = """## How to use this skill

1. Read the page as it is today; don't work from memory. From the repository root:
   `python3 .claude/skills/hig/scripts/hig_page.py {slug}`
   For one section: `python3 .claude/skills/hig/scripts/hig_page.py {slug} --section <anchor>`, with an anchor from the list below.
2. Apply it to the task. The guidelines are written for Apple's frameworks; the PDM app is Expo / React Native, so translate each rule with [the hig skill's Expo notes](../hig/expo.md) and keep to [docs/frontend](../../../docs/frontend/README.md) (UI system, accessibility, platform adapters).
3. Cite the section link for every rule you apply and every finding you report.
"""


def article_skill(node, by_slug, by_url):
    page = node["page"]
    front = ["---", f"name: {node['name']}", f"description: {yaml_str(description(node, by_slug))}"]
    if not phone_relevant(node):
        front.append("disable-model-invocation: true")
    lines = front + ["---", "",
             "<!-- Generated by .claude/skills/hig/scripts/build_skills.py. Don't edit by hand; rerun the script. -->", "",
             f"# {node['title']} (Apple Human Interface Guidelines)", "",
             f"**Page:** {page_url(node['slug'])}  ",
             f"**Where:** {breadcrumb(node)}  ",
             f"**Parent skill:** `/{node['chain'][-1]['name'] if node['chain'] else 'hig'}`  ",
             platform_line(platforms(page))]
    changed = last_changed(page)
    if changed:
        lines[-1] += "  "
        lines.append(f"**Last changed by Apple:** {changed}")
    lines += ["", HOW_TO.format(slug=node["slug"]), "## Sections", ""]
    for level, title, anchor in sections(page):
        indent = "  " if level == 3 else ""
        lines.append(f"{indent}- [{title}]({page_url(node['slug'])}#{anchor}) (`{anchor}`)")
    res = resources(page)
    for group in ("Related", "Developer documentation", "Videos"):
        items = res.pop(group, [])
        if items:
            lines += ["", f"## {group}", ""]
            for title, url in items:
                lines.append(f"- [{title}]({url})" + skill_note(url, by_url))
    for group, items in res.items():
        if items:
            lines += ["", f"## {group}", ""] + [f"- [{t}]({u})" + skill_note(u, by_url) for t, u in items]
    return "\n".join(lines) + "\n"


def skill_note(url, by_url):
    node = by_url.get(url.split("#", 1)[0])
    return f" · `/{node['name']}`" if node else ""


def group_skill(node, by_slug):
    lines = ["---", f"name: {node['name']}", f"description: {yaml_str(description(node, by_slug))}", "---", "",
             "<!-- Generated by .claude/skills/hig/scripts/build_skills.py. Don't edit by hand; rerun the script. -->", "",
             f"# {node['title']} (Apple Human Interface Guidelines)", "",
             f"**Page:** {page_url(node['slug'])}  ",
             f"**Where:** {breadcrumb(node)}  ",
             f"**Parent skill:** `/{node['chain'][-1]['name'] if node['chain'] else 'hig'}`", "",
             "## How to use this skill", "",
             "Pick the pages below that the task touches and use their skills; each one reads its page live and lists its sections. "
             "Pages whose platforms don't include iOS or iPadOS rarely apply to the PDM app.", "",
             "## Pages in this group", ""]
    for child in node["children"]:
        c = by_slug[child]
        kind = "group" if c["role"] != "article" else ", ".join(PLATFORM_NAMES.get(p, p) for p in platforms(c["page"])) or "all platforms"
        lines.append(f"- [{c['title']}]({page_url(child)}) · `/{c['name']}` ({kind})")
    return "\n".join(lines) + "\n"


def index_file(nodes, by_slug):
    lines = ["# Human Interface Guidelines: every page and its skill", "",
             "<!-- Generated by .claude/skills/hig/scripts/build_skills.py. Don't edit by hand; rerun the script. -->", "",
             f"{len(nodes) - 1} pages and groups, read from {page_url('')} on {time.strftime('%Y-%m-%d')}. "
             "Each line gives the page, its skill and its platforms (iOS and iPadOS are the ones the PDM app's iPhone build uses).", ""]

    def walk(slug, depth):
        n = by_slug[slug]
        if slug:
            plats = ", ".join(PLATFORM_NAMES.get(p, p) for p in platforms(n["page"]))
            extra = f" ({plats})" if n["role"] == "article" and plats else ""
            label = f"**{n['title']}**" if n["role"] != "article" else n["title"]
            lines.append(f"{'  ' * depth}- {label}: [{page_url(slug).split('/')[-1]}]({page_url(slug)}) · `/{n['name']}`{extra}")
        for c in n["children"]:
            walk(c, depth + (1 if slug else 0))

    walk("", 0)
    return "\n".join(lines) + "\n"


def main():
    nodes = crawl()
    by_slug = {n["slug"]: n for n in nodes}
    by_url = {page_url(n["slug"]): n for n in nodes}
    too_long = [n["name"] for n in nodes if len(n["name"]) > MAX_NAME]
    if too_long:
        sys.exit(f"Skill names over {MAX_NAME} characters: {too_long}")
    for d in os.listdir(SKILLS):
        if d.startswith("hig-") and os.path.isdir(os.path.join(SKILLS, d)):
            shutil.rmtree(os.path.join(SKILLS, d))
    for n in nodes[1:]:
        text = article_skill(n, by_slug, by_url) if n["role"] == "article" else group_skill(n, by_slug)
        os.makedirs(os.path.join(SKILLS, n["name"]), exist_ok=True)
        with open(os.path.join(SKILLS, n["name"], "SKILL.md"), "w") as f:
            f.write(text)
    with open(os.path.join(SKILLS, "hig", "index.md"), "w") as f:
        f.write(index_file(nodes, by_slug))
    articles = sum(1 for n in nodes if n["role"] == "article")
    print(f"{len(nodes) - 1} skills: {articles} pages and {len(nodes) - 1 - articles} groups, plus the hig index")


if __name__ == "__main__":
    main()

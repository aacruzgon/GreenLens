#!/usr/bin/env python3
"""Print a page of Apple's Human Interface Guidelines as Markdown, read live.

Usage:
  python3 .claude/skills/hig/scripts/hig_page.py <slug> [--section <anchor>]

<slug> is the last part of the page's URL ("charts" for
https://developer.apple.com/design/human-interface-guidelines/charts).
--section prints one section only, by its anchor ("Best-practices").

The page is fetched from the JSON that developer.apple.com renders the
site from, so the text is always current and none of it is stored here.
Standard library only.
"""
import argparse
import json
import sys
import urllib.error
import urllib.request

SITE = "https://developer.apple.com"
DATA = SITE + "/tutorials/data"
HIG = "/design/human-interface-guidelines"


def fetch(slug):
    path = HIG if slug in ("", "human-interface-guidelines") else f"{HIG}/{slug}"
    req = urllib.request.Request(DATA + path + ".json", headers={"User-Agent": "pdm-hig-skill"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


def absolute(url):
    """A full URL that survives Markdown: parentheses encoded, Apple's internal CMS host replaced."""
    url = url if url.startswith("http") else SITE + url
    url = url.replace("https://devcms.apple.com/", SITE + "/")
    return url.replace("(", "%28").replace(")", "%29")


class Renderer:
    def __init__(self, page):
        self.refs = page.get("references", {})

    def ref(self, identifier):
        r = self.refs.get(identifier, {})
        title = r.get("title") or identifier.rsplit("/", 1)[-1]
        url = r.get("url")
        return f"[{title}]({absolute(url)})" if url else title

    def inline(self, items):
        out = []
        for i in items or []:
            t = i.get("type")
            if t == "text":
                out.append(i.get("text", ""))
            elif t == "strong":
                out.append("**" + self.inline(i.get("inlineContent")) + "**")
            elif t == "emphasis":
                out.append("*" + self.inline(i.get("inlineContent")) + "*")
            elif t == "codeVoice":
                out.append("`" + i.get("code", "") + "`")
            elif t == "reference":
                out.append(self.ref(i.get("identifier", "")))
            elif t == "link":
                out.append(f"[{i.get('title') or i.get('destination')}]({i.get('destination')})")
            elif t == "image":
                out.append(self.image(i.get("identifier")))
            else:
                out.append(self.inline(i.get("inlineContent")))
        return "".join(out)

    def image(self, identifier):
        alt = self.refs.get(identifier, {}).get("alt")
        return f"[Image: {alt}]" if alt else ""

    def blocks(self, items, indent=""):
        lines = []
        for b in items or []:
            t = b.get("type")
            if t == "heading":
                lines += ["", "#" * b.get("level", 2) + " " + b.get("text", ""), ""]
            elif t == "paragraph":
                lines += [indent + self.inline(b.get("inlineContent")), ""]
            elif t in ("unorderedList", "orderedList"):
                for n, item in enumerate(b.get("items", []), 1):
                    body = self.blocks(item.get("content"), indent + "   ").strip()
                    mark = f"{n}." if t == "orderedList" else "-"
                    lines.append(f"{indent}{mark} {body}")
                lines.append("")
            elif t == "aside":
                name = b.get("name") or b.get("style", "note").capitalize()
                body = self.blocks(b.get("content")).strip().replace("\n", "\n> ")
                lines += [f"> **{name}:** {body}", ""]
            elif t == "table":
                lines += self.table(b) + [""]
            elif t == "row":
                for col in b.get("columns", []):
                    lines.append(self.blocks(col.get("content"), indent).rstrip())
                lines.append("")
            elif t == "tabNavigator":
                for tab in b.get("tabs", []):
                    lines += [f"{indent}**{tab.get('title', '')}:**", self.blocks(tab.get("content"), indent).rstrip(), ""]
            elif t == "links":
                lines += [f"{indent}- {self.ref(i)}" for i in b.get("items", [])] + [""]
            elif t == "small":
                lines += [indent + self.inline(b.get("inlineContent")), ""]
            elif t == "image":
                alt = self.image(b.get("identifier"))
                if alt:
                    lines += [indent + alt, ""]
            elif t == "video":
                continue
            else:
                text = self.inline(b.get("inlineContent")) or self.blocks(b.get("content"), indent)
                if text.strip():
                    lines += [indent + text, ""]
        return "\n".join(lines)

    def table(self, b):
        rows = []
        for row in b.get("rows", []):
            cells = [self.blocks(cell).strip().replace("\n", " ").replace("|", "\\|") for cell in row]
            rows.append("| " + " | ".join(cells) + " |")
        if rows and b.get("header") == "row":
            rows.insert(1, "|" + "---|" * len(b["rows"][0]))
        elif rows:
            rows.insert(0, "|" + " |" * len(b["rows"][0]))
            rows.insert(1, "|" + "---|" * len(b["rows"][0]))
        return rows


def section(content, anchor):
    start = next((i for i, b in enumerate(content) if b.get("type") == "heading" and b.get("anchor") == anchor), None)
    if start is None:
        anchors = [b.get("anchor") for b in content if b.get("type") == "heading"]
        sys.exit(f"No section '{anchor}'. Anchors on this page: {', '.join(a for a in anchors if a)}")
    level = content[start].get("level", 2)
    end = next((i for i in range(start + 1, len(content))
                if content[i].get("type") == "heading" and content[i].get("level", 2) <= level), len(content))
    return content[start:end]


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("slug")
    p.add_argument("--section")
    a = p.parse_args()
    try:
        page = fetch(a.slug.strip("/").rsplit("/", 1)[-1])
    except urllib.error.HTTPError as e:
        sys.exit(f"{e.code} for '{a.slug}': check the slug against the hig skill's index.")
    r = Renderer(page)
    meta = page.get("metadata", {})
    url = SITE + (HIG if a.slug in ("", "human-interface-guidelines") else f"{HIG}/{a.slug}")
    content = [b for s in page.get("primaryContentSections", []) for b in s.get("content", [])]
    if a.section:
        content = section(content, a.section)
        url += "#" + a.section
    print(f"# {meta.get('title', a.slug)}\n")
    print(f"Source: {url}")
    platforms = meta.get("customMetadata", {}).get("supported-platforms")
    if platforms:
        print(f"Platforms: {platforms}")
    if not a.section:
        print("\n" + r.inline(page.get("abstract")))
    print(r.blocks(content))
    if not a.section and not content:
        for s in page.get("topicSections", []):
            print(f"\n## {s.get('title') or 'Pages'}\n")
            for i in s.get("identifiers", []):
                print(f"- {r.ref(i)}")
    print(f"\n{page.get('legalNotices', {}).get('copyright', '').replace('&copy;', '©')}")


if __name__ == "__main__":
    main()

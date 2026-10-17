#!/usr/bin/env python3
"""Send a Discord alert to each team member a GitHub comment newly mentions.

usage: discord_mentions.py "$GITHUB_EVENT_PATH"

Run by .github/workflows/discord-mentions.yml on issue_comment events (docs/infra/discord-mentions.md).
The routing comes from the environment, for each PERSON in PEOPLE:

  GH_USER_<PERSON>          their GitHub username (a repository variable)
  DISCORD_WEBHOOK_<PERSON>  the webhook of their Discord channel (a repository secret)

The comment, the title and every other field of the event are untrusted: they are parsed and sent to
Discord as JSON, never run and never printed, so a comment can't inject a workflow command into the
log. Webhook URLs are never printed either, not even inside an exception's text. Exits 1 if an alert
could not be delivered or a mentioned person's webhook is missing or malformed; the other alerts are
still sent.
"""
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

PEOPLE = ("SARAH", "SAM", "ALAN", "MARK")

# GitHub usernames: letters, digits and single hyphens, up to 39 characters. Underscores are allowed
# for Enterprise Managed Users (`name_shortcode`).
USERNAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]{0,38}")

# An @mention: not preceded by a character that would make it part of an email address, a URL path
# or a longer word, and taken whole (the second lookahead stops the match backing off to a prefix,
# so `@alanc` is never `@alan`). `@org/team` is a team mention, not a user. A trailing `_` or `-`
# is punctuation, as in `_@mark_` (italics) or `@mark-`: mentions() strips it.
MENTION = re.compile(r"(?<![A-Za-z0-9.+\-/=@])@([A-Za-z0-9_-]+)(?![A-Za-z0-9_-]|/[A-Za-z0-9])")

FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
# A code span may wrap lines but never crosses a blank line, so it is matched per paragraph.
INLINE_CODE = re.compile(r"(?<!`)(`+)(?!`)(.+?)(?<!`)\1(?!`)", re.DOTALL)
PARAGRAPH_BREAK = re.compile(r"\n[ \t]*\n")
HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)

WEBHOOK_HOSTS = {"discord.com", "discordapp.com", "ptb.discord.com", "canary.discord.com"}
WEBHOOK_PATH = re.compile(r"/api(?:/v\d+)?/webhooks/\d+/[A-Za-z0-9_-]+")

EXCERPT_CHARS = 600
ATTEMPTS = 3
MAX_WAIT_S = 30.0


@dataclass(frozen=True)
class Person:
    key: str  # SARAH, SAM, ...
    login: str  # lower case
    webhook: str  # may be empty: checked only when the person is mentioned


def strip_code(body: str) -> str:
    """Remove what GitHub doesn't treat as a mention: fenced code, code spans and HTML comments.

    Indented code blocks are kept: telling them from a list item's continuation needs a full
    Markdown parser, and the cost of keeping them is an extra alert, never a missed one.
    """
    kept, fence = [], None
    for line in body.splitlines():
        m = FENCE.match(line)
        if fence is None:
            if m:
                fence = m.group(1)
            else:
                kept.append(line)
        elif m and m.group(1)[0] == fence[0] and len(m.group(1)) >= len(fence) and not line.strip(" \t`~"):
            fence = None  # the closing fence: the same character, at least as long, nothing after it
    text = "\n".join(kept)
    text = HTML_COMMENT.sub(" ", text)
    return "\n\n".join(INLINE_CODE.sub(" ", p) for p in PARAGRAPH_BREAK.split(text))


def mentions(body: str | None) -> set[str]:
    """Every username mentioned in a comment, in lower case."""
    if not body:
        return set()
    found = (m.rstrip("_-").lower() for m in MENTION.findall(strip_code(body)))
    return {m for m in found if m}


def new_mentions(event: dict) -> set[str]:
    """The mentions an event adds: all of them for a new comment, only new ones for an edit."""
    body = event.get("comment", {}).get("body")
    action = event.get("action")
    if action == "created":
        return mentions(body)
    if action == "edited":
        changes = event.get("changes") or {}
        if "body" not in changes:
            return set()  # the body didn't change
        return mentions(body) - mentions(changes["body"].get("from"))
    return set()


def people(env: dict[str, str]) -> tuple[list[Person], list[str]]:
    """The configured people, and a warning for each one that can't be routed."""
    out, warnings = [], []
    for key in PEOPLE:
        login = env.get(f"GH_USER_{key}", "").strip().removeprefix("@")
        webhook = env.get(f"DISCORD_WEBHOOK_{key}", "").strip()
        if not login:
            warnings.append(f"GH_USER_{key} is not set; {key} gets no alerts")
        elif not USERNAME.fullmatch(login):
            warnings.append(f"GH_USER_{key} is not a valid GitHub username; {key} gets no alerts")
        else:
            out.append(Person(key, login.lower(), webhook))
    return out, warnings


def webhook_problem(url: str) -> str | None:
    """Why a webhook URL can't be used, without repeating the URL; None if it's fine."""
    if not url:
        return "is not set"
    parts = urllib.parse.urlsplit(url)
    if parts.path.rstrip("/").endswith(("/github", "/slack")):
        return "ends in /github or /slack; remove that suffix, since this workflow sends Discord's own format"
    if parts.scheme != "https" or parts.hostname not in WEBHOOK_HOSTS or not WEBHOOK_PATH.fullmatch(parts.path):
        return "is not a Discord webhook URL (https://discord.com/api/webhooks/<id>/<token>)"
    return None


def escape(text: str) -> str:
    """Escape Discord markdown in a short plain-text value, such as a username or a title."""
    return re.sub(r"([\\*_~`|\[\]])", r"\\\1", text)


def truncate(text: str, limit: int) -> str:
    return text if len(text) <= limit else text[: limit - 1].rstrip() + "…"


def excerpt(body: str) -> str:
    text = re.sub(r"\n{3,}", "\n\n", (body or "").strip())
    return truncate(text, EXCERPT_CHARS) or "(empty comment)"


def message(event: dict, person: Person) -> dict:
    """The Discord webhook payload for one alert."""
    comment, issue, repo = event["comment"], event["issue"], event["repository"]
    kind = "pull request" if issue.get("pull_request") else "issue"
    edited = event.get("action") == "edited"
    # An edit's mention is the editor's, who may not be the comment's author.
    user = event.get("sender", {}) if edited else comment.get("user", {})
    author = user.get("login") or comment.get("user", {}).get("login", "someone")
    number = issue.get("number")
    return {
        # No pings at all: not @everyone, @here, roles or users, whatever the comment says.
        "allowed_mentions": {"parse": []},
        "content": truncate(
            f"**{escape(author)}** mentioned {escape(person.login)} in "
            f"{'an edited comment' if edited else 'a comment'} on {escape(repo['full_name'])}#{number}",
            2000,
        ),
        "embeds": [
            {
                "title": truncate(f"{kind.capitalize()} #{number}: {issue.get('title', '')}", 256),
                "url": comment["html_url"],
                "author": {"name": truncate(author, 256), "url": user.get("html_url")},
                "description": excerpt(comment.get("body", "")),
                "fields": [
                    {"name": "Repository", "value": f"[{escape(repo['full_name'])}]({repo['html_url']})", "inline": True},
                    {"name": kind.capitalize(), "value": f"[#{number}]({issue['html_url']})", "inline": True},
                    {"name": "Comment", "value": f"[Open the comment]({comment['html_url']})", "inline": True},
                ],
                "footer": {"text": "Edited comment" if edited else "New comment"},
                "timestamp": comment.get("updated_at" if edited else "created_at"),
            }
        ],
    }


def send(url: str, payload: dict, user_agent: str, opener=urllib.request.urlopen, sleep=time.sleep) -> str | None:
    """POST one alert, retrying rate limits and server errors. Returns None, or why it failed.

    The reason never includes the URL: only status codes, Discord's error message and exception
    types (http.client and urllib put the URL in some exceptions' text, so that text isn't used).
    """
    data = json.dumps(payload).encode()
    headers = {"Content-Type": "application/json", "User-Agent": user_agent}
    reason = "not attempted"
    for attempt in range(1, ATTEMPTS + 1):
        wait = None
        try:
            with opener(urllib.request.Request(url, data=data, headers=headers, method="POST"), timeout=15):
                return None
        except urllib.error.HTTPError as e:
            detail = discord_error(e)
            reason = f"HTTP {e.code}{': ' + detail['message'] if detail.get('message') else ''}"
            if e.code == 429:
                wait = seconds(detail.get("retry_after") or (e.headers or {}).get("Retry-After"))
            elif e.code >= 500:
                wait = float(2**attempt)
            else:
                if e.code in (401, 404):
                    reason += " (the webhook was deleted, or the secret holds the wrong URL)"
                return reason
        except Exception as e:  # network errors and timeouts; never str(e), see above
            reason = f"network error ({type(e).__name__})"
            wait = float(2**attempt)
        if attempt < ATTEMPTS:
            sleep(min(wait, MAX_WAIT_S))
    return f"{reason}, after {ATTEMPTS} attempts"


def seconds(value) -> float:
    try:
        return max(float(value), 0.0)
    except (TypeError, ValueError):
        return 1.0


def discord_error(e: urllib.error.HTTPError) -> dict:
    try:
        body = json.loads(e.read() or b"{}")
    except Exception:  # a bad body or a broken read (http.client.IncompleteRead is neither of the usual)
        return {}
    if not isinstance(body, dict):
        return {}
    msg = str(body.get("message", ""))[:200]
    return {"message": re.sub(r"[^\w .,'()/-]", "", msg), "retry_after": body.get("retry_after")}


def annotate(level: str, text: str) -> None:
    """A workflow-command annotation. Only our own text and Discord's sanitised errors reach it."""
    text = text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    print(f"::{level} title=Discord mention alert::{text}")


def run(event: dict, env: dict[str, str], opener=urllib.request.urlopen, sleep=time.sleep) -> int:
    mentioned = new_mentions(event)
    configured, warnings = people(env)
    for w in warnings:
        annotate("warning", w)
    targets = [p for p in configured if p.login in mentioned]
    if not targets:
        print("No new mention of a configured username; nothing to send.")
        return 0

    repo_url = event.get("repository", {}).get("html_url", "https://github.com")
    user_agent = f"DiscordBot ({repo_url}, 1)"
    failed, summary = 0, []
    for person in targets:
        problem = webhook_problem(person.webhook)
        if problem:
            annotate("error", f"DISCORD_WEBHOOK_{person.key} {problem}; {person.key}'s alert was not sent")
            failed += 1
            summary.append(f"- {person.key}: not sent, DISCORD_WEBHOOK_{person.key} {problem}")
            continue
        reason = send(person.webhook, message(event, person), user_agent, opener, sleep)
        if reason:
            annotate("error", f"Discord refused {person.key}'s alert: {reason}")
            failed += 1
            summary.append(f"- {person.key}: not sent, {reason}")
        else:
            print(f"Sent {person.key}'s alert.")
            summary.append(f"- {person.key}: sent")

    if path := env.get("GITHUB_STEP_SUMMARY"):
        with open(path, "a", encoding="utf-8") as f:
            f.write("### Discord mention alerts\n" + "\n".join(summary) + "\n")
    return 1 if failed else 0


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__.splitlines()[2], file=sys.stderr)
        return 2
    with open(argv[1], encoding="utf-8") as f:
        event = json.load(f)
    return run(event, dict(os.environ))


if __name__ == "__main__":
    sys.exit(main(sys.argv))

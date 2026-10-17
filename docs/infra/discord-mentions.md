# Mention alerts on Discord

> **Status:** Draft v0.1.1, 2026-10-17. v0.1.1: after the audit of #34, code spans never cross a blank line, a closing fence may end in a tab, `_@mark_` and `@mark-` match, an edit's alert names the editor, and section 1 says indented code blocks still match. v0.1: first version of the workflow that posts a Discord alert to a team member's channel when a GitHub comment mentions them. Its tests have run against a mocked Discord; nothing has been sent to a real channel.

When an issue comment or a pull request conversation comment mentions a team member's GitHub username, [`.github/workflows/discord-mentions.yml`](../../.github/workflows/discord-mentions.yml) posts an alert in that person's Discord channel. The work is done by [`.github/scripts/discord_mentions.py`](../../.github/scripts/discord_mentions.py). It belongs with CI in [infra 2](README.md#2-ci).

## Contents

1. [What it does](#1-what-it-does)
2. [Setup](#2-setup)
3. [What is covered](#3-what-is-covered)
4. [Security](#4-security)
5. [Testing](#5-testing)

---

## 1. What it does

| Person | Channel | Username variable | Webhook secret |
|---|---|---|---|
| Sarah | `#github-sarah` | `GH_USER_SARAH` | `DISCORD_WEBHOOK_SARAH` |
| Sam | `#github-sam` | `GH_USER_SAM` | `DISCORD_WEBHOOK_SAM` |
| Alan | `#github-alan` | `GH_USER_ALAN` | `DISCORD_WEBHOOK_ALAN` |
| Mark | `#github-mark` | `GH_USER_MARK` | `DISCORD_WEBHOOK_MARK` |

- **One alert per person mentioned.** A comment that mentions two of them sends two alerts, one to each channel. Mentioning someone twice sends one alert. Comments that mention no one in the table send nothing.
- **Matching.** A mention is `@` and the whole username, in any case: `@Mark` matches `mark`, `@markus` and `@mark-2` don't. It doesn't match inside an email address (`mark@example.com`), a URL path, or a team mention (`@org/team`). Like GitHub, it ignores mentions in fenced code blocks, inline code and HTML comments; a trailing `_` or `-` is punctuation, so `_@mark_` matches `mark`. Unlike GitHub, it still matches inside an indented (four-space) code block, which costs an extra alert at worst.
- **Edits.** An edit sends an alert only to people the edit newly mentions: the script compares the new text with the old, which GitHub sends with the event. Fixing a typo in a comment that already mentions someone sends nothing.
- **The alert** shows the repository, whether it is an issue or a pull request, its number and title, the comment's author (for an edit, the person who edited it), the first 600 characters of the comment, and links to the comment, the issue or pull request, and the repository. It pings no one: the message sets `allowed_mentions` to none, so `@everyone`, `@here`, roles and users in a comment or title stay plain text.
- **Failures.** Discord's rate limits (HTTP 429) and server errors are retried up to 3 times. If an alert still fails, or a mentioned person's webhook secret is missing or malformed, the job fails with an error annotation naming the person and the HTTP status, and the other alerts are still sent. A person whose username variable is empty gets no alerts, and the run shows a warning.

## 2. Setup

### Create a webhook for each channel

For each of the four channels, a Discord server admin (or anyone with *Manage Webhooks*):

1. Opens the channel's settings (the gear beside its name) → **Integrations** → **Webhooks** → **New Webhook**.
2. Names it, for example `GitHub mentions`. The channel is already set to the one being edited.
3. Clicks **Copy Webhook URL**. It looks like `https://discord.com/api/webhooks/<id>/<token>`.

**Don't append `/github` to these URLs.** That suffix makes Discord expect GitHub's own webhook format; this workflow sends Discord's format, and the script refuses a URL that ends in `/github` or `/slack`. The URL is a credential: anyone who has it can post in the channel. Paste it only into the secret below.

### Add the secrets and variables

In the repository on GitHub: **Settings** → **Secrets and variables** → **Actions**.

- On the **Secrets** tab, **New repository secret** four times: `DISCORD_WEBHOOK_SARAH`, `DISCORD_WEBHOOK_SAM`, `DISCORD_WEBHOOK_ALAN`, `DISCORD_WEBHOOK_MARK`, each set to that person's webhook URL.
- On the **Variables** tab, **New repository variable** four times: `GH_USER_SARAH`, `GH_USER_SAM`, `GH_USER_ALAN`, `GH_USER_MARK`, each set to that person's exact GitHub username (a leading `@` is ignored). Ask each person for theirs; don't guess it from a first name.

The variables are named `GH_USER_*` because GitHub refuses a variable or secret whose name starts with `GITHUB_`. From a terminal, a repository admin can do the same with `gh secret set DISCORD_WEBHOOK_SARAH` (it prompts for the value, so the URL stays out of the shell history) and `gh variable set GH_USER_SARAH --body <username>`.

### Activate and test

GitHub runs an `issue_comment` workflow only from the default branch, so nothing happens until the workflow is merged into `main` through a pull request. Then:

1. Open **Actions** → **Discord mentions** to see that it is listed.
2. On any issue or pull request, comment `test for @<username>` for one person. Their channel should get one alert within a minute, and the run should show `Sent <PERSON>'s alert.`
3. Edit the comment's other text: no second alert. Edit it to add a second person: only that person gets an alert.
4. Comment something with no mention, or with `mark@example.com`: the run is skipped or sends nothing.

A failed run's error annotation says what went wrong: `HTTP 404: Unknown Webhook` means the webhook was deleted or the secret holds the wrong URL; `is not set` means a secret is missing.

### Avoid duplicate alerts

A GitHub-to-Discord webhook set up directly on the repository (**Settings** → **Webhooks**, a URL ending in `/github`) posts every comment it is subscribed to on its own. If it posts to one of the four channels with the **Issue comments** event, that channel gets two messages for a comment: GitHub's and this workflow's. For each such webhook, either:

- edit it (**Let me select individual events**) and untick **Issue comments**, keeping the events it is still wanted for; or
- point it at a different channel, such as a shared `#github` feed; or
- delete it, if this workflow replaces it.

## 3. What is covered

| Comment | Supported | Notes |
|---|---|---|
| Issue comment | Yes | New and edited |
| Pull request conversation comment | Yes | New and edited. Works on pull requests from forks too, because `issue_comment` always runs in this repository with its secrets |
| Inline review comment (on a diff line) | No | See below |
| Review summary (the text of an approve, comment or request-changes review) | No | See below |
| Comments on commits, discussions | No | Not requested |

**Why reviews aren't covered yet.** Review comments and summaries arrive as `pull_request_review_comment` and `pull_request_review` events. GitHub runs those from the pull request's merge commit, not from `main`, so the workflow file and script that run would be the pull request's own version, which the pull request can change. For a pull request from a fork, GitHub gives the run no secrets, so it couldn't reach any webhook. Adding them later means: a second workflow on those two events that checks out `main`'s script (never the pull request's), accepts that fork pull requests send nothing, and maps their payloads (`review.body`, `pull_request.title`) to the same alert. Until then, people see review mentions through GitHub's own notifications.

## 4. Security

- **Token:** `contents: read`, only for the checkout of the script. The alerts go out through the webhooks.
- **No pull request code:** the job checks out only `discord_mentions.py`, from the default branch, without keeping the token in git's config.
- **Comment text is data:** it reaches the script only through the event file (`$GITHUB_EVENT_PATH`), never through an `${{ }}` expression in a shell command, and is sent to Discord as JSON. The script never prints a comment, a title or any other text from the event, so a comment can't inject a workflow command such as `::add-mask::` into the log. The workflow's `if:` reads the comment only to skip comments with no `@`.
- **Webhook URLs:** only in secrets. GitHub masks them in logs, and the script never prints them, not even in an exception's text: a failure is reported by person, HTTP status and Discord's error message, or by exception type for a network error.

## 5. Testing

```sh
python3 -m unittest discover -s .github/scripts -v
```

The tests replace the HTTP call with a fake Discord, so nothing is sent. They cover matching (case, prefixes, email addresses, URL paths, team mentions, code), routing (one alert per person, unconfigured people), edits (only new mentions), the alert's content and `allowed_mentions`, retries on 429, 5xx and network errors, a refused `/github` URL, and that no webhook URL or comment text reaches the log. The `infra` job of [`ci.yml`](../../.github/workflows/ci.yml) runs them whenever `.github/` changes.

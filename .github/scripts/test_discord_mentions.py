"""Tests for discord_mentions.py. Discord is mocked: nothing here sends a request.

usage: python3 -m unittest discover -s .github/scripts
"""
import http.client
import io
import json
import os
import tempfile
import unittest
import unittest.mock
import urllib.error

import discord_mentions as dm

HOOK = "https://discord.com/api/webhooks/{}/tok-{}_secret"
ENV = {
    "GH_USER_SARAH": "sarah-k",
    "GH_USER_SAM": "SamDev",
    "GH_USER_ALAN": "@aacruzgon",  # a leading @ is tolerated
    "GH_USER_MARK": "mark",
    **{f"DISCORD_WEBHOOK_{k}": HOOK.format(i, k.lower()) for i, k in enumerate(dm.PEOPLE, 1)},
}


def event(body, action="created", old=None, pr=True):
    e = {
        "action": action,
        "comment": {
            "body": body,
            "html_url": "https://github.com/aacruzgon/GreenLens/pull/7#issuecomment-1",
            "created_at": "2026-10-17T12:00:00Z",
            "updated_at": "2026-10-17T12:05:00Z",
            "user": {"login": "commenter", "html_url": "https://github.com/commenter"},
        },
        "issue": {
            "number": 7,
            "title": "Fix @everyone <@123> things",
            "html_url": "https://github.com/aacruzgon/GreenLens/pull/7",
        },
        "repository": {"full_name": "aacruzgon/GreenLens", "html_url": "https://github.com/aacruzgon/GreenLens"},
    }
    if pr:
        e["issue"]["pull_request"] = {"url": "…"}
    if action == "edited" and old is not None:
        e["changes"] = {"body": {"from": old}}
    return e


class FakeDiscord:
    """Stands in for urlopen: records each request and replays scripted responses per URL."""

    def __init__(self, responses=None):
        self.requests = []
        self.responses = responses or {}  # url -> list of status codes or exceptions, used in order

    def __call__(self, request, timeout):
        self.requests.append(request)
        queue = self.responses.get(request.full_url, [])
        outcome = queue.pop(0) if queue else 204
        if isinstance(outcome, Exception):
            raise outcome
        if outcome >= 400:
            body = {"message": "You are being rate limited.", "retry_after": 0.5} if outcome == 429 else {"message": "Unknown Webhook", "code": 10015}
            raise urllib.error.HTTPError(request.full_url, outcome, "error", {}, io.BytesIO(json.dumps(body).encode()))
        return io.BytesIO(b"")

    def urls(self):
        return [r.full_url for r in self.requests]

    def payloads(self):
        return [json.loads(r.data) for r in self.requests]


def run(e, env=ENV, discord=None):
    discord = discord or FakeDiscord()
    sleeps = []
    code = dm.run(e, env, opener=discord, sleep=sleeps.append)
    return code, discord, sleeps


class Mentions(unittest.TestCase):
    def test_matches_whole_usernames_case_insensitively(self):
        self.assertEqual(dm.mentions("cc @Sarah-K and @MARK, thanks @samdev."), {"sarah-k", "mark", "samdev"})

    def test_ignores_prefixes_and_longer_names(self):
        self.assertEqual(dm.mentions("@markus @mark-2 @mark_x @marks"), {"markus", "mark-2", "mark_x", "marks"})
        self.assertNotIn("mark", dm.mentions("@markus @mark-2 @mark_x @marks"))

    def test_ignores_email_addresses_and_url_paths(self):
        self.assertEqual(dm.mentions("mail mark@example.com or x.mark@y.org, see https://x.com/@mark"), set())

    def test_mentions_at_line_start_and_after_punctuation(self):
        self.assertEqual(dm.mentions("@mark\n(@sarah-k) **@samdev**: >@aacruzgon"), {"mark", "sarah-k", "samdev", "aacruzgon"})

    def test_team_mentions_are_not_user_mentions(self):
        self.assertEqual(dm.mentions("ping @aacruzgon/backend"), set())

    def test_ignores_code_and_html_comments(self):
        body = "see `@mark` and ``x @sam y``\n```\n@sarah-k\n```\n<!-- @aacruzgon -->\n~~~~\n@x\n~~~~\nbut @real"
        self.assertEqual(dm.mentions(body), {"real"})

    def test_unclosed_fence_hides_the_rest(self):
        self.assertEqual(dm.mentions("@a\n```\n@b"), {"a"})

    def test_code_spans_never_cross_a_blank_line(self):
        self.assertEqual(dm.mentions("Press ` then type.\n\n@mark see `x`"), {"mark"})
        self.assertEqual(dm.mentions("a `code\n@sam` span"), set())

    def test_closing_fence_may_end_in_a_tab(self):
        self.assertEqual(dm.mentions("```\n@mark\n```\t\n@sam"), {"sam"})

    def test_trailing_underscore_or_hyphen_is_punctuation(self):
        self.assertEqual(dm.mentions("_@mark_ and _cc @sam_ and thanks @alan-"), {"mark", "sam", "alan"})
        self.assertEqual(dm.mentions("@mark_x"), {"mark_x"})


class Routing(unittest.TestCase):
    def test_one_alert_per_mentioned_person(self):
        code, discord, _ = run(event("@sarah-k @MARK please look; also @nobody"))
        self.assertEqual(code, 0)
        self.assertEqual(sorted(discord.urls()), sorted([ENV["DISCORD_WEBHOOK_SARAH"], ENV["DISCORD_WEBHOOK_MARK"]]))

    def test_repeated_mentions_send_once(self):
        _, discord, _ = run(event("@mark @mark @Mark"))
        self.assertEqual(discord.urls(), [ENV["DISCORD_WEBHOOK_MARK"]])

    def test_leading_at_in_the_variable_is_tolerated(self):
        _, discord, _ = run(event("@aacruzgon"))
        self.assertEqual(discord.urls(), [ENV["DISCORD_WEBHOOK_ALAN"]])

    def test_no_matching_mention_sends_nothing(self):
        for body in ("no mentions", "@someone-else", "mark@example.com", "`@mark`", ""):
            code, discord, _ = run(event(body))
            self.assertEqual((code, discord.requests), (0, []), body)

    def test_unconfigured_person_is_skipped(self):
        env = {**ENV, "GH_USER_SAM": ""}
        code, discord, _ = run(event("@samdev @mark"), env)
        self.assertEqual((code, discord.urls()), (0, [ENV["DISCORD_WEBHOOK_MARK"]]))

    def test_invalid_username_variable_is_skipped(self):
        env = {**ENV, "GH_USER_MARK": "mark smith"}
        code, discord, _ = run(event("@mark"), env)
        self.assertEqual((code, discord.requests), (0, []))

    def test_deleted_comments_send_nothing(self):
        _, discord, _ = run(event("@mark", action="deleted"))
        self.assertEqual(discord.requests, [])


class Edits(unittest.TestCase):
    def test_edit_that_adds_a_mention_alerts_only_the_new_person(self):
        _, discord, _ = run(event("@mark and now @sarah-k", "edited", old="@mark"))
        self.assertEqual(discord.urls(), [ENV["DISCORD_WEBHOOK_SARAH"]])
        self.assertEqual(discord.payloads()[0]["embeds"][0]["footer"]["text"], "Edited comment")

    def test_edit_of_unrelated_text_sends_nothing(self):
        _, discord, _ = run(event("@mark, typo fixed", "edited", old="@mark, typo fixd"))
        self.assertEqual(discord.requests, [])

    def test_changing_a_mentions_case_is_not_new(self):
        _, discord, _ = run(event("@MARK", "edited", old="@mark"))
        self.assertEqual(discord.requests, [])

    def test_moving_a_mention_out_of_code_is_new(self):
        _, discord, _ = run(event("@mark", "edited", old="`@mark`"))
        self.assertEqual(discord.urls(), [ENV["DISCORD_WEBHOOK_MARK"]])

    def test_edit_credits_the_editor(self):
        e = event("@mark", "edited", old="hi")
        e["sender"] = {"login": "maintainer", "html_url": "https://github.com/maintainer"}
        _, discord, _ = run(e)
        payload = discord.payloads()[0]
        self.assertIn("**maintainer** mentioned mark", payload["content"])
        self.assertEqual(payload["embeds"][0]["author"]["name"], "maintainer")

    def test_edit_without_a_body_change_sends_nothing(self):
        _, discord, _ = run(event("@mark", "edited"))
        self.assertEqual(discord.requests, [])


class Payload(unittest.TestCase):
    def setUp(self):
        _, discord, _ = run(event("hi @mark " + "x" * 2000, pr=True))
        self.payload = discord.payloads()[0]
        self.embed = self.payload["embeds"][0]

    def test_never_pings(self):
        self.assertEqual(self.payload["allowed_mentions"], {"parse": []})

    def test_holds_repo_title_number_author_excerpt_and_link(self):
        text = json.dumps(self.payload)
        self.assertIn("aacruzgon/GreenLens", text)
        self.assertEqual(self.embed["title"], "Pull request #7: Fix @everyone <@123> things")
        self.assertEqual(self.embed["author"]["name"], "commenter")
        self.assertEqual(self.embed["url"], "https://github.com/aacruzgon/GreenLens/pull/7#issuecomment-1")
        self.assertIn("[Open the comment](https://github.com/aacruzgon/GreenLens/pull/7#issuecomment-1)", text)
        self.assertEqual(self.embed["footer"]["text"], "New comment")

    def test_excerpt_is_truncated(self):
        self.assertEqual(len(self.embed["description"]), dm.EXCERPT_CHARS)
        self.assertTrue(self.embed["description"].endswith("…"))

    def test_issue_kind(self):
        _, discord, _ = run(event("@mark", pr=False))
        self.assertTrue(discord.payloads()[0]["embeds"][0]["title"].startswith("Issue #7:"))

    def test_markdown_in_names_is_escaped(self):
        e = event("@mark")
        e["comment"]["user"]["login"] = "under_score"
        _, discord, _ = run(e)
        self.assertIn("**under\\_score**", discord.payloads()[0]["content"])


class Delivery(unittest.TestCase):
    def test_retries_a_rate_limit_then_succeeds(self):
        url = ENV["DISCORD_WEBHOOK_MARK"]
        code, discord, sleeps = run(event("@mark"), discord=FakeDiscord({url: [429]}))
        self.assertEqual((code, len(discord.requests), sleeps), (0, 2, [0.5]))

    def test_retries_server_errors_and_network_errors(self):
        url = ENV["DISCORD_WEBHOOK_MARK"]
        code, discord, _ = run(event("@mark"), discord=FakeDiscord({url: [502, urllib.error.URLError("down")]}))
        self.assertEqual((code, len(discord.requests)), (0, 3))

    def test_failure_is_reported_without_the_url_and_others_still_sent(self):
        sarah, mark = ENV["DISCORD_WEBHOOK_SARAH"], ENV["DISCORD_WEBHOOK_MARK"]
        out = io.StringIO()
        with tempfile.NamedTemporaryFile("r", suffix=".md") as summary:
            env = {**ENV, "GITHUB_STEP_SUMMARY": summary.name}
            with unittest.mock.patch("sys.stdout", out):
                code, discord, _ = run(event("@sarah-k @mark"), env, FakeDiscord({mark: [404]}))
            summary_text = summary.read()
        self.assertEqual(code, 1)
        self.assertIn(sarah, discord.urls())
        log = out.getvalue() + summary_text
        self.assertIn("::error title=Discord mention alert::Discord refused MARK's alert: HTTP 404: Unknown Webhook", log)
        for k in dm.PEOPLE:
            self.assertNotIn(f"tok-{k.lower()}", log)
            self.assertNotIn("/api/webhooks/", log)

    def test_exception_text_is_never_logged(self):
        url = ENV["DISCORD_WEBHOOK_MARK"]
        leaky = ValueError(f"bad url {url}")
        out = io.StringIO()
        with unittest.mock.patch("sys.stdout", out):
            code, _, _ = run(event("@mark"), discord=FakeDiscord({url: [leaky, leaky, leaky]}))
        self.assertEqual(code, 1)
        self.assertIn("network error (ValueError), after 3 attempts", out.getvalue())
        self.assertNotIn("tok-mark", out.getvalue())

    def test_a_broken_error_body_still_reports_and_sends_the_rest(self):
        class Broken(io.BytesIO):
            def read(self, *_):
                raise http.client.IncompleteRead(b"")

        mark = ENV["DISCORD_WEBHOOK_MARK"]
        err = urllib.error.HTTPError(mark, 400, "bad", {}, Broken())
        with unittest.mock.patch("sys.stdout", io.StringIO()):
            code, discord, _ = run(event("@sarah-k @mark"), discord=FakeDiscord({mark: [err]}))
        self.assertEqual(code, 1)
        self.assertIn(ENV["DISCORD_WEBHOOK_SARAH"], discord.urls())

    def test_github_suffix_is_refused_before_sending(self):
        env = {**ENV, "DISCORD_WEBHOOK_MARK": ENV["DISCORD_WEBHOOK_MARK"] + "/github"}
        out = io.StringIO()
        with unittest.mock.patch("sys.stdout", out):
            code, discord, _ = run(event("@mark"), env)
        self.assertEqual((code, discord.requests), (1, []))
        self.assertIn("ends in /github", out.getvalue())
        self.assertNotIn("tok-mark", out.getvalue())

    def test_missing_or_foreign_webhook_is_refused(self):
        for bad in ("", "https://example.com/api/webhooks/1/abc", "http://discord.com/api/webhooks/1/abc"):
            env = {**ENV, "DISCORD_WEBHOOK_MARK": bad}
            with unittest.mock.patch("sys.stdout", io.StringIO()):
                code, discord, _ = run(event("@mark"), env)
            self.assertEqual((code, discord.requests), (1, []), bad)

    def test_comment_text_never_reaches_the_log(self):
        out = io.StringIO()
        with unittest.mock.patch("sys.stdout", out):
            run(event("@mark ::add-mask::x\n::error::injected"))
        self.assertNotIn("injected", out.getvalue())
        self.assertNotIn("add-mask", out.getvalue())


class Main(unittest.TestCase):
    def test_reads_the_event_file(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump(event("no mentions"), f)
        try:
            with unittest.mock.patch.dict(os.environ, ENV), unittest.mock.patch("sys.stdout", io.StringIO()):
                self.assertEqual(dm.main(["discord_mentions.py", f.name]), 0)
        finally:
            os.unlink(f.name)


if __name__ == "__main__":
    unittest.main()

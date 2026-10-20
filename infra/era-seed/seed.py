"""Inject the Maple Row Farm fixture into the demo simulators' default tenant, then exit
(docs/infra/README.md section 4, the `demo` profile; docs/EON E9.1).

    python /src/seed.py --fixture /fixture --slack http://era-slack:8080 --drive http://era-drive:8080

Run by the one-shot `era-seed` service, the only service that holds SIMCORE_ADMIN_TOKEN (from
.env.era-seed). `api` never holds it, and without it the simulators ignore `X-Sim-Tenant`, so the
farm goes into the tenant every token without a tenant reads: `default`.

Steps, for each simulator:
1. Check that `/_admin` without the token answers 401, or stop: an open admin API means the token
   didn't reach the simulator.
2. Reset the default tenant, then inject the fixture. A message injected twice is listed twice
   (EON section 3), so every run starts from the generated company and the result is the same.
3. Read the farm back through the public API with the demo connectors' tokens (`xoxb-era-demo`,
   `ya29.era-demo`, api's farm_sync/seed.rs) and compare the counts with the fixture's.

Standard library only. It never prints the token. Any failure exits 1, which keeps `api` down.
"""

import argparse
import json
import os
import pathlib
import sys
import urllib.error
import urllib.parse
import urllib.request

TENANT = "default"
SLACK_TOKEN = "xoxb-era-demo"
DRIVE_TOKEN = "ya29.era-demo"
FOLDER = "application/vnd.google-apps.folder"


def call(method, url, token=None, body=None):
    """One request; returns (status, parsed JSON or None)."""
    headers = {"content-type": "application/json"} if body is not None else {}
    if token:
        headers["authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            status, raw = resp.status, resp.read()
    except urllib.error.HTTPError as err:
        status, raw = err.code, err.read()
    try:
        return status, json.loads(raw)
    except ValueError:
        return status, None


def fail(msg):
    print(f"era-seed: {msg}", file=sys.stderr)
    sys.exit(1)


def seed(name, base, fixture_path, admin):
    status, _ = call("GET", f"{base}/_admin/tenants")
    if status != 401:
        fail(f"{name}: /_admin without the admin token answered {status}, not 401")
    for action, body in (("reset", None), ("inject", fixture_path.read_bytes())):
        status, out = call("POST", f"{base}/_admin/tenants/{TENANT}/{action}", admin, body)
        if status != 200 or not out or out.get("status") != "ok":
            fail(f"{name}: {action} answered {status} {out}")
    print(f"era-seed: {name}: reset and injected {fixture_path.name} into tenant {TENANT}")


def slack_history(base, channel):
    count, cursor = 0, ""
    while True:
        query = urllib.parse.urlencode({"channel": channel, "limit": 200, "cursor": cursor})
        status, out = call("GET", f"{base}/api/conversations.history?{query}", SLACK_TOKEN)
        if status != 200 or not out or not out.get("ok"):
            fail(f"slack: conversations.history {channel} answered {status} {out}")
        count += len(out.get("messages", []))
        cursor = (out.get("response_metadata") or {}).get("next_cursor") or ""
        if not cursor:
            return count


def drive_children(base, folder):
    count, page = 0, ""
    while True:
        params = {"q": f"'{folder}' in parents", "pageSize": 100}
        if page:
            params["pageToken"] = page
        query = urllib.parse.urlencode(params)
        status, out = call("GET", f"{base}/drive/v3/files?{query}", DRIVE_TOKEN)
        if status != 200 or not out:
            fail(f"drive: files.list in {folder} answered {status} {out}")
        count += len(out.get("files", []))
        page = out.get("nextPageToken") or ""
        if not page:
            return count


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--fixture", type=pathlib.Path, required=True)
    parser.add_argument("--slack", required=True)
    parser.add_argument("--drive", required=True)
    args = parser.parse_args()

    admin = os.environ.get("SIMCORE_ADMIN_TOKEN", "")
    if len(admin) < 32 or admin == "change-me":
        fail("SIMCORE_ADMIN_TOKEN is unset or too short (openssl rand -hex 32)")
    slack_file, drive_file = args.fixture / "slack.json", args.fixture / "google-drive.json"
    slack_fix, drive_fix = json.loads(slack_file.read_text()), json.loads(drive_file.read_text())

    seed("slack", args.slack.rstrip("/"), slack_file, admin)
    seed("drive", args.drive.rstrip("/"), drive_file, admin)

    for channel in (c["id"] for c in slack_fix["channels"]):
        want = sum(1 for m in slack_fix["messages"] if m["channel"] == channel)
        got = slack_history(args.slack.rstrip("/"), channel)
        if got != want:
            fail(f"slack: {channel} holds {got} messages, the fixture {want}")
        print(f"era-seed: slack: {channel} holds {got} messages")
    entries = drive_fix.get("files", []) + drive_fix.get("sheets", [])
    for folder in (f["id"] for f in entries if f.get("mimeType") == FOLDER):
        want = sum(1 for f in entries if folder in (f.get("parents") or []))
        got = drive_children(args.drive.rstrip("/"), folder)
        if got != want:
            fail(f"drive: {folder} holds {got} files, the fixture {want}")
        print(f"era-seed: drive: {folder} holds {got} files")


if __name__ == "__main__":
    main()

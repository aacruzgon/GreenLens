#!/usr/bin/env python3
"""Creates the demo garden, Maple Row Farm, through the PDM API (EON E9.0).

Signs in as a fixed Clerk test user, demo+clerk_test@example.com, the way the smoke script does
(tools/smoke/README.md), and makes it the owner of a garden named after the fixture's
garden.json, with one plant per bed: its name, its species resolved through GET /species, and its
status. Safe to run again: it reuses the user's garden of that name and the plants it already
holds, creates what is missing, corrects a bed's status, and gives a bed created without a
species its species once the knowledge base holds it. It prints the garden's id as
ERA_DEMO_GARDEN_ID=<id>, the value api's start-up seeding needs for the demo connectors (E9.2).

usage (from backend/):
    python3 tools/demo/demo_garden.py [--base-url http://localhost:8080]
        [--env-file ../infra/dev/.env] [--garden tests/fixtures/era/maple-row/garden.json]
        [--lat 43.55 --lon -80.25]

CLERK_SECRET_KEY comes from the environment or --env-file; it must start with sk_test_.
Neither the key nor a token is ever printed. Exit code 0 when the garden matches the fixture.
"""
import argparse
import json
import sys
import urllib.parse
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "smoke"))
from smoke import Api, Clerk, die, info, load_env  # noqa: E402

DEMO_EMAIL = "demo+clerk_test@example.com"
# Sign-up fields an instance may require; unique per user, so not the smoke user's. 555-0100 to
# 555-0199 are Clerk's test phone numbers, which never send an SMS.
DEMO_USER_FIELDS = {"username": "pdm_demo", "phone_number": ["+15555550101"]}


def pages(api, path, params=None):
    """Every item of a paginated list ({items, next_cursor?})."""
    params = dict(params or {}, limit=100)
    while True:
        out = api.call("GET", path + "?" + urllib.parse.urlencode(params))
        yield from out["items"]
        if not out.get("next_cursor"):
            return
        params["cursor"] = out["next_cursor"]


def species_id(api, scientific_name):
    """The knowledge base's id for a scientific name, or None when it doesn't hold the species."""
    q = urllib.parse.urlencode({"q": scientific_name, "limit": 100})
    for s in api.call("GET", f"/species?{q}")["items"]:
        if s["scientific_name"].lower() == scientific_name.lower():
            return s["id"]
    return None


def main():
    ap = argparse.ArgumentParser(description="Create the Maple Row Farm demo garden (EON E9.0).")
    ap.add_argument("--base-url", default="http://localhost:8080")
    ap.add_argument("--env-file", default="../infra/dev/.env")
    ap.add_argument("--garden", default="tests/fixtures/era/maple-row/garden.json",
                    help="the fixture's garden.json")
    ap.add_argument("--lat", type=float, help="with --lon, instead of the fixture's city")
    ap.add_argument("--lon", type=float)
    args = ap.parse_args()
    if (args.lat is None) != (args.lon is None):
        die("--lat and --lon go together")

    fixture = json.loads(Path(args.garden).read_text())
    if fixture.get("format") != 1:
        die(f"{args.garden}: expected format 1")
    secret = load_env(args.env_file, "CLERK_SECRET_KEY")
    if not secret:
        die(f"CLERK_SECRET_KEY is not set and not in {args.env_file}")
    api = Api(args.base_url, Clerk(secret, DEMO_EMAIL, DEMO_USER_FIELDS))
    if not api.healthy():
        die(f"{args.base_url}/healthz doesn't answer 200")

    name = fixture["garden"]["name"]
    owned = [g for g in pages(api, "/gardens") if g["name"] == name and g["my_role"] == "owner"]
    if len(owned) > 1:
        die(f"{DEMO_EMAIL} owns {len(owned)} gardens named {name!r}; delete all but one")
    if owned:
        garden = owned[0]
        info(f"garden {name!r} exists")
    else:
        location = ({"lat": args.lat, "lon": args.lon} if args.lat is not None
                    else fixture["garden"]["location"])
        garden = api.call("POST", "/gardens", {"name": name, "location": location},
                          expect=(201,), idem=True)
        info(f"created garden {name!r}")
    want_tz = fixture["garden"].get("time_zone")
    if want_tz and garden["time_zone"] != want_tz:
        info(f"note: the garden's time zone is {garden['time_zone']}, the fixture's {want_tz}")

    plants = {p["name"]: p for p in pages(api, "/plants", {"garden_id": garden["id"]})}
    for bed in fixture["plants"]:
        plant = plants.get(bed["name"])
        sid = species_id(api, bed["species"]["scientific_name"])
        if sid is None:
            info(f"{bed['name']}: {bed['species']['scientific_name']} isn't in the knowledge base; "
                 "left without a species")
        if plant is None:
            body = {"garden_id": garden["id"], "name": bed["name"]}
            if sid:
                body["species_id"] = sid
            plant = api.call("POST", "/plants", body, expect=(201,), idem=True)
            info(f"created {bed['name']}")
        elif sid and plant.get("species") is None and not plant.get("custom_species_name"):
            # Created before the knowledge base held the species: confirm it now.
            plant = api.call("POST", f"/plants/{plant['id']}/species", {"species_id": sid})
            info(f"{bed['name']}: species {bed['species']['scientific_name']}")
        if plant["status"] != bed["status"]:
            plant = api.call("PATCH", f"/plants/{plant['id']}", {"status": bed["status"]})
            info(f"{bed['name']}: status {bed['status']}")
        species = (plant.get("species") or {}).get("scientific_name", "no species")
        info(f"  {bed['name']}: {species}, {plant['species_status']}, {plant['status']}")

    print(f"ERA_DEMO_GARDEN_ID={garden['id']}")


if __name__ == "__main__":
    main()

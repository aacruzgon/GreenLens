#!/usr/bin/env python3
"""Creates the demo garden, Maple Row Farm, through the PDM API (EON E9.0).

Signs in as a fixed Clerk test user, demo+clerk_test@example.com, the way the smoke script does
(tools/smoke/README.md), and makes it the owner of a garden named after the fixture's
garden.json, with one plant per bed: its name, its species resolved through GET /species, and its
status. Safe to run again: it reuses the user's garden of that name and the plants it already
holds, creates what is missing, corrects a bed's status, and gives a bed created without a
species its species once the knowledge base holds it. It never changes a species a bed already
has: a change would re-run the bed's latest scan. It prints the garden's id as
ERA_DEMO_GARDEN_ID=<id>, the value api's start-up seeding needs for the demo connectors (E9.2).

usage:
    python3 backend/tools/demo/demo_garden.py [--base-url http://localhost:8080]
        [--env-file infra/dev/.env] [--garden backend/tests/fixtures/era/maple-row/garden.json]
        [--lat 43.55 --lon -80.25]

CLERK_SECRET_KEY comes from the environment or --env-file; it must start with sk_test_.
Neither the key nor a token is ever printed. Exit code 0 once the garden and every bed exist with
their statuses; a bed whose species is missing or differs from the fixture is listed as a warning
(bed 3's matters for the demo scan, E9.3). Exit code 1 on any error.
"""
import argparse
import json
import sys
import urllib.parse
from pathlib import Path

HERE = Path(__file__).resolve().parent
BACKEND = HERE.parent.parent
sys.path.insert(0, str(HERE.parent / "smoke"))
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


def species_id(api, species):
    """The knowledge base's id for a fixture species, or None when it doesn't hold it.

    A scientific name can cover several species (Brassica oleracea is kale, cabbage and
    broccoli), so the common name decides between them. A species recorded by its genus only
    (squash is Cucurbita, for Zucchini's Cucurbita pepo) counts when one of its names contains
    the fixture's common name: `q` matches every name, though the list shows only the first
    common name, so two such species can't be told apart and give None (a warning) instead.
    """
    sci = species["scientific_name"].lower()
    matches = [s for s in pages(api, "/species", {"q": species["scientific_name"]})
               if s["scientific_name"].lower() == sci]
    if not matches:
        genus = sci.split()[0]
        matches = [s for s in pages(api, "/species", {"q": species["common_name"]})
                   if s["scientific_name"].lower() == genus]
        return matches[0]["id"] if len(matches) == 1 else None
    if len(matches) > 1:
        common = species["common_name"].lower()
        matches = [s for s in matches if (s.get("common_name") or "").lower() == common] or matches
    if len(matches) > 1:
        die(f"{species['scientific_name']} matches {', '.join(s['id'] for s in matches)}; "
            f"none or several are named {species['common_name']!r}")
    return matches[0]["id"] if matches else None


def main():
    ap = argparse.ArgumentParser(description="Create the Maple Row Farm demo garden (EON E9.0).")
    ap.add_argument("--base-url", default="http://localhost:8080")
    ap.add_argument("--env-file", default=str(BACKEND.parent / "infra/dev/.env"))
    ap.add_argument("--garden", default=str(BACKEND / "tests/fixtures/era/maple-row/garden.json"),
                    help="the fixture's garden.json")
    ap.add_argument("--lat", type=float, help="with --lon, instead of the fixture's city")
    ap.add_argument("--lon", type=float)
    args = ap.parse_args()
    if (args.lat is None) != (args.lon is None):
        die("--lat and --lon go together")

    if not Path(args.garden).is_file():
        die(f"{args.garden}: no such file")
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
        if args.lat is not None:
            info("note: --lat and --lon are ignored for an existing garden")
    else:
        location = ({"lat": args.lat, "lon": args.lon} if args.lat is not None
                    else fixture["garden"]["location"])
        garden = api.call("POST", "/gardens", {"name": name, "location": location},
                          expect=(201,), idem=True)
        info(f"created garden {name!r}")
    want_tz = fixture["garden"].get("time_zone")
    if want_tz and garden["time_zone"] != want_tz:
        info(f"note: the garden's time zone is {garden['time_zone']}, the fixture's {want_tz}")

    plants = {}
    for p in pages(api, "/plants", {"garden_id": garden["id"]}):
        if p["name"] in plants:
            die(f"the garden has two plants named {p['name']!r}; delete one")
        plants[p["name"]] = p
    warnings = []
    for bed in fixture["plants"]:
        plant = plants.get(bed["name"])
        sid = species_id(api, bed["species"])
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
        species = plant.get("species")
        shown = f"{species['id']} ({species['common_name']})" if species else "no species"
        info(f"  {bed['name']}: {shown}, {plant['species_status']}, {plant['status']}")
        want = f"{bed['species']['common_name']} ({bed['species']['scientific_name']})"
        if sid is None:
            warnings.append(f"{bed['name']}: the knowledge base doesn't hold {want}")
        elif species is None or species["id"] != sid or plant["species_status"] != "confirmed":
            warnings.append(f"{bed['name']}: {shown}, {plant['species_status']}, not {sid}; change "
                            "it in the app (a species change re-runs the bed's latest scan)")

    for w in warnings:
        info(f"warning: {w}")
    print(f"ERA_DEMO_GARDEN_ID={garden['id']}")


if __name__ == "__main__":
    main()

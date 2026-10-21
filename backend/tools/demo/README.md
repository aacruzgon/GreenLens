# demo

`demo_garden.py` creates the demo garden, Maple Row Farm, through the API ([EON E9.0](../../../docs/EON/README.md#e9-the-demo-garden), [connectors 9.4](../../../docs/connectors/README.md#94-the-demo)). Python 3, standard library only. Run it from `backend/`.

It signs in the way the smoke script does ([smoke](../smoke/README.md)): Clerk's Backend API mints a session token for a fixed test user, `demo+clerk_test@example.com` (username `pdm_demo`, test phone `+15555550101`), which the script creates on its first run. It is a separate user from the smoke script's, so smoke runs never touch the demo garden. `CLERK_SECRET_KEY` must start with `sk_test_`; the key and the tokens are never printed.

## What it does

From the fixture's [`garden.json`](../../tests/fixtures/era/maple-row/garden.json):

1. `GET /gardens`: reuses the user's own garden named `Maple Row Farm`, or `POST /gardens` with the fixture's city (`Guelph, ON`), or `--lat`/`--lon`. Two such gardens stop the script.
2. For each bed, `GET /species?q=` resolves the scientific name to a knowledge-base species. A bed is created with it (`POST /plants`, so the species is confirmed), or without a species when the knowledge base doesn't hold it. A bed created without one gets it on a later run, once the knowledge base holds it (`POST /plants/{id}/species`). The status is set with `PATCH /plants/{id}` (bed 3 is `sick`).
3. It prints `ERA_DEMO_GARDEN_ID=<id>`. Put that line in `.env.api` and restart `api`: with `CONNECTOR_KEY` and the two `ERA_*` bases set, its start-up seeding gives the garden its two `era` connectors (EON E4.6, E9.2).

Every run is safe to repeat and gives the same garden id. It writes no scans, memories or photos: the fixture's scans and memories go through the store in the evaluation only, and the bed 3 photo that gives early blight on real models is a scan the demo takes (E9.3).

## Signing in as the demo owner

An address with `+clerk_test` is a test address: Clerk sends no email and the verification code is always `424242` ([Clerk, test emails and phones](https://clerk.com/docs/guides/development/testing/test-emails-and-phones)). That works on instances in test mode, which every development instance is by default. A production instance has it off, and Clerk discourages turning it on, so there the demo owner needs a real address.

## Options

| Option | Meaning |
|---|---|
| `--base-url` | default `http://localhost:8080`; the public control-plane URL for the rig |
| `--env-file` | where `CLERK_SECRET_KEY` is read when it isn't in the environment; default `../infra/dev/.env` |
| `--garden` | the fixture; default `tests/fixtures/era/maple-row/garden.json` |
| `--lat`, `--lon` | both or neither: coordinates instead of the fixture's city |

## Checked

✓ 2026-10-20, on a laptop against a local `api` (the dev compose stack, the Clerk development instance):
- The first run created the Clerk user (the instance asked for a phone number, which the script supplies), the garden and the six beds, with bed 3 `sick`, and printed the id. The knowledge base was empty, so every bed was left without a species.
- A second run reused the garden and the six beds and printed the same id.
- With `species:tomato` added to the local database and `api` restarted (it loads the knowledge base at start), a third run gave bed 3 its species: `Solanum lycopersicum`, `confirmed`, `sick`.
- A throwaway copy of the fixture under another name, with `--lat`/`--lon`, created bed 3 with its species at once; the garden was then deleted.
- With `CONNECTOR_KEY`, both `ERA_*` bases and the printed id, `api` logged "era connectors seeded", and `GET /gardens/{id}/connectors` as the demo owner listed the Slack and Drive connectors, both `active` with `endpoint: era`.

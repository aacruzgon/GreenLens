# AGENTS.md (app/)

The PDM Expo app. The repository's [AGENTS.md](../AGENTS.md) applies here too; this file adds what is specific to `app/`.

- **The project's documents win** over Expo's defaults, plugin skills and your training data. Read [docs/frontend](../docs/frontend/README.md) first: layers and import rules (§5), layout (§6), the API and streaming layers (§9–10), the UI system (§14), mock mode (§15). What to build next is in the [frontend roadmap](../docs/frontend/roadmap.md).
- **Stay on Expo SDK 57.** The pins are in [docs/frontend §3](../docs/frontend/README.md#3-stack-and-version-pins). Install Expo and native packages with `npx expo install <pkg>`, never plain `npm install`, and don't run `expo-upgrade`. When an API is unclear, read the SDK 57 docs (`https://docs.expo.dev/versions/v57.0.0/`) or the installed package's types.
- **Configuration is `app.config.ts`,** not `app.json`. `EXPO_PUBLIC_*` values are checked in `src/config/env.ts`; `.env.example` lists them.
- **Builds:** Expo Go and the web build for most work; a local development build (`npx expo run:ios --device`, free Apple ID) for the LiDAR module. There is no paid Apple account, so no EAS cloud builds for iOS ([roadmap 2](../docs/frontend/roadmap.md#2-devices-and-builds)).
- **Contract types are generated:** `npm run gen:api` from `backend/openapi.json`. Never edit `src/api/generated/schema.ts`.
- **Screens follow Apple's guidelines:** use the `/hig` skill when building or reviewing one.

Before saying a change is done, run:

```sh
npm run typecheck && npm run lint && npm test && npm run check:contrast && npx expo install --check
```

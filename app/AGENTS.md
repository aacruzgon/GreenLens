# app/AGENTS.md

The PDM app: Expo SDK 57, React Native 0.86, Expo Router, `@clerk/expo` 4.x, TypeScript strict. Built for the iPhone; the web build comes last.

Read first:

- [docs/frontend/roadmap.md](../docs/frontend/roadmap.md): the slice you are working on, its work and exit checks.
- [docs/frontend/README.md](../docs/frontend/README.md): layers and import rules (§5), layout (§6), the api layer (§9–10), mock mode (§15).
- [docs/frontend/screens.md](../docs/frontend/screens.md) for what a screen contains, and [docs/design/design.md](../docs/design/design.md) §3.1 (Conservatory) and §6 for how it looks.
- The `/hig` skill before designing or reviewing a screen.

Expo changes every SDK: check the v57 docs (`https://docs.expo.dev/versions/v57.0.0/`) instead of memory.

## Commands

```sh
npx expo install <pkg>      # never a plain npm install for Expo or native packages
npm start                   # Expo Go; press i for the iOS simulator
npm run gen:api             # regenerate src/api/generated/schema.ts after a backend contract change
npm run typecheck && npm run lint && npm test && npm run check:contrast
```

`.env` comes from `.env.example`. `EXPO_PUBLIC_API_MODE=mock` with an empty Clerk key runs with no backend and a demo sign-in.

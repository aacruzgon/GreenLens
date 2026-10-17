// docs/frontend 5 and 16: the layer import rules are lint errors.
const { defineConfig } = require("eslint/config");
const expoConfig = require("eslint-config-expo/flat");

const zone = (target, from, message) => ({ target, from, message });

module.exports = defineConfig([
  expoConfig,
  {
    ignores: ["dist/*", ".expo/*", "src/api/generated/*"],
  },
  {
    rules: {
      "import/no-restricted-paths": [
        "error",
        {
          basePath: __dirname,
          zones: [
            zone("./src/app", "./src/api", "Routes render features; only features call the api layer."),
            zone("./src/app", "./src/platform", "Routes render features; only features use platform adapters."),
            zone("./src/features/*/model", "./src/api", "model/ is pure: no I/O."),
            zone("./src/features/*/model", "./src/platform", "model/ is pure: no device access."),
            zone("./src/features/*/model", "./src/ui", "model/ is pure: no React."),
            zone("./src/api", "./src/features", "api never imports features."),
            zone("./src/api", "./src/ui", "api never imports ui."),
            zone("./src/api", "./src/platform", "api never imports platform."),
            zone("./src/platform", "./src/features", "platform never imports features."),
            zone("./src/platform", "./src/api", "platform never imports api."),
            zone("./src/platform", "./src/ui", "platform never imports ui."),
            zone("./src/ui", "./src/features", "ui never imports features."),
            zone("./src/ui", "./src/api", "ui never imports api."),
            zone("./src/ui", "./src/platform", "ui never imports platform."),
            zone("./src/domain", "./src/features", "domain imports only lib."),
            zone("./src/domain", "./src/ui", "domain imports only lib."),
            zone("./src/domain", "./src/platform", "domain imports only lib."),
            { target: "./src/lib", from: "./src", except: ["./lib"], message: "lib imports nothing else in src." },
          ],
        },
      ],
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@/features/*/*"],
              message: "Import another feature through its index.ts only.",
            },
            {
              group: ["@clerk/*"],
              message: "Only features/auth talks to Clerk.",
            },
          ],
        },
      ],
    },
  },
  {
    files: ["src/features/auth/**", "src/app/_layout.tsx"],
    rules: { "no-restricted-imports": "off" },
  },
]);

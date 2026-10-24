// Flat config. The import-boundary zones enforce docs/frontend/README.md §5.
const { defineConfig } = require("eslint/config");
const expoConfig = require("eslint-config-expo/flat");

// `@/x` resolves to src/x through tsconfig paths (eslint-import-resolver-typescript), so zone
// paths are plain src-relative directories.
const zone = (target, from, message, except) => ({
  target,
  from,
  message,
  ...(except ? { except } : {}),
});

module.exports = defineConfig([
  expoConfig,
  {
    ignores: ["src/api/generated/**", "dist/**", ".expo/**", "node_modules/**", "coverage/**"],
  },
  {
    settings: {
      "import/resolver": {
        typescript: { project: "./tsconfig.json", alwaysTryTypes: true },
        node: true,
      },
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    rules: {
      // An import named after a built-in hides it in the whole module, compiled code included: the React
      // Compiler opens each component with `Symbol.for(...)`, so `Image as Symbol` crashed the scan screen on
      // iOS while Jest, which doesn't compile, passed (review of #130).
      "no-restricted-syntax": [
        "error",
        {
          selector:
            ":matches(ImportSpecifier, ImportDefaultSpecifier, ImportNamespaceSpecifier)[local.name=/^(Symbol|Object|Array|Function|Promise|Map|Set|WeakMap|WeakSet|Error|TypeError|JSON|Math|Date|Number|String|Boolean|BigInt|RegExp|Proxy|Reflect|globalThis|undefined|NaN|Infinity)$/]",
          message: "Don't name an import after a JavaScript built-in: it hides the global in this module, and compiled code uses it. Alias it (`Image as SFSymbol`).",
        },
      ],
      "import/no-restricted-paths": [
        "error",
        {
          basePath: __dirname,
          zones: [
            zone("./src/app", ["./src/api", "./src/platform"], "Routes import features and ui, never api or platform (README 5)."),
            zone(
              "./src/api",
              ["./src/features", "./src/ui", "./src/platform"],
              "api imports domain and lib only (README 5).",
            ),
            zone("./src/platform", ["./src/features", "./src/api", "./src/ui"], "platform imports domain and lib only (README 5)."),
            zone("./src/ui", ["./src/api", "./src/features", "./src/platform"], "ui imports domain and lib only (README 5)."),
            zone(
              "./src/domain",
              [
                "./src/api",
                "./src/app",
                "./src/config",
                "./src/features",
                "./src/platform",
                "./src/ui",
              ],
              "domain imports lib only (README 5).",
              // domain/types.ts re-exports the generated types
              ["./generated"],
            ),
            zone(
              "./src/lib",
              [
                "./src/api",
                "./src/app",
                "./src/config",
                "./src/domain",
                "./src/features",
                "./src/platform",
                "./src/ui",
              ],
              "lib imports nothing in src (README 5).",
            ),
            // Local native modules are reached only through src/platform (README 5 and 13).
            ...["./src/app", "./src/features", "./src/ui", "./src/api", "./src/domain", "./src/lib", "./src/config"].map((target) =>
              zone(target, ["./modules"], "Only src/platform imports a local native module (README 5, 13)."),
            ),
            // features/*/model: pure TypeScript
            zone(
              "./src/features/*/model/**",
              ["./src/api", "./src/platform"],
              "A feature's model/ is pure: no api or platform (README 5).",
            ),
          ],
        },
      ],
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/[^/]+/.+",
              message: "Import another feature through its index.ts only: '@/features/x' (README 5).",
            },
            {
              group: ["@clerk/*"],
              message: "@clerk/* is imported only inside src/features/auth and src/app/_layout.tsx (README 8).",
            },
          ],
        },
      ],
    },
  },
  // model/ may not import react or react-native
  {
    files: ["src/features/*/model/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            { regex: "^@/features/[^/]+/.+", message: "Import another feature through its index.ts only." },
            { group: ["@clerk/*"], message: "@clerk/* stays in features/auth." },
            { regex: "^(react|react-native|react-dom)($|/)", message: "A feature's model/ is pure TypeScript: no react (README 5)." },
          ],
        },
      ],
    },
  },
  // Clerk is allowed in features/auth and the root layout; a feature may reach into its own files.
  {
    files: ["src/features/auth/**/*.{ts,tsx}", "src/app/_layout.tsx"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [{ regex: "^@/features/[^/]+/.+", message: "Import another feature through its index.ts only." }],
        },
      ],
    },
  },
  // A feature's own deep paths (relative imports) are fine; the pattern above only matches the alias.
  {
    files: ["*.config.js", "scripts/**"],
    languageOptions: { globals: { __dirname: "readonly", module: "writable", require: "readonly", process: "readonly" } },
  },
  {
    files: ["**/*.test.{ts,tsx}", "jest.setup.ts"],
    languageOptions: { globals: { jest: "readonly" } },
  },
]);

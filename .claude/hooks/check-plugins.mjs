#!/usr/bin/env node
// SessionStart hook: tells a teammate which plugins this project enables but their machine lacks.
//
// Committing a plugin in .claude/settings.json turns it on for everyone, but Claude Code doesn't
// download it; each person installs it once. This hook compares the project's enabledPlugins with
// the local install record and, when one is missing, shows the install command and asks Claude to
// offer to run it. A plugin set to false in .claude/settings.local.json counts as opted out.
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const project = process.env.CLAUDE_PROJECT_DIR || process.cwd();
const readJson = (path) => {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch {
    return {};
  }
};

const settings = readJson(join(project, ".claude", "settings.json"));
const shared = settings.enabledPlugins || {};
const marketplaces = settings.extraKnownMarketplaces || {};
const local = readJson(join(project, ".claude", "settings.local.json")).enabledPlugins || {};
const wanted = Object.keys(shared).filter((id) => shared[id] === true && local[id] !== false);

const pluginsDir =
  process.env.CLAUDE_CODE_PLUGIN_CACHE_DIR ||
  join(process.env.CLAUDE_CONFIG_DIR || join(homedir(), ".claude"), "plugins");
const recordPath = join(pluginsDir, "installed_plugins.json");
const installed = existsSync(recordPath) ? readJson(recordPath).plugins || {} : {};
const missing = wanted.filter((id) => !installed[id]);

if (missing.length === 0) process.exit(0);

// A plugin from a marketplace the project declares is installed with --marketplace, which adds that
// marketplace first when this machine doesn't know it yet (Claude Code v2.1.292 or later).
const installCommand = (id) => {
  const [name, marketplace] = id.split("@");
  const source = marketplaces[marketplace]?.source || {};
  const from = source.source === "github" ? source.repo : source.source === "git" ? source.url : null;
  return from
    ? `claude plugin install ${name} --marketplace ${from} --scope project`
    : `claude plugin install ${id} --scope project`;
};
const commands = missing.map(installCommand);
const list = missing.join(", ");
const many = missing.length > 1;
process.stdout.write(
  JSON.stringify({
    systemMessage:
      `This project uses the Claude Code plugin${many ? "s" : ""} ${list}, ` +
      `not installed on this machine. Install from your shell:\n  ${commands.join("\n  ")}\n` +
      `then run /reload-plugins. To opt out of one, set it to false in .claude/settings.local.json.`,
    hookSpecificOutput: {
      hookEventName: "SessionStart",
      additionalContext:
        `The project's .claude/settings.json enables ${list}, but ${many ? "they aren't" : "it isn't"} installed on this machine. ` +
        `Early in your first reply, tell the user and offer to install ${many ? "them" : "it"} by running: ${commands.join(" && ")} ` +
        `(then they run /reload-plugins). Install only if they agree.`,
    },
  }),
);

#!/usr/bin/env node

const pkg = require("../package.json");

const arg = process.argv[2];

function help() {
  console.log(`1Context ${pkg.version}

Own your context. An engine for agentic work.

Usage:
  1context --version
  1context doctor
  1context paths
  1context --help
`);
}

function paths() {
  console.log(`1Context paths:

Config: ~/Library/Application Support/1Context/config
Data:   ~/Library/Application Support/1Context
Cache:  ~/Library/Caches/1Context
Logs:   ~/Library/Logs/1Context

No directories were created by this command.
`);
}

function doctor() {
  console.log(`1Context doctor

✓ CLI installed
✓ Version: ${pkg.version}
✓ Homebrew-compatible command available

Status:
  Bootstrap preview. Product runtime is not installed yet.

Privacy:
  This command makes no network calls and collects no telemetry.

Next:
  Follow https://github.com/hapticasensorics/1context for public releases.
`);
}

if (arg === "--version" || arg === "-v" || arg === "version") {
  console.log(pkg.version);
} else if (arg === "doctor") {
  doctor();
} else if (arg === "paths") {
  paths();
} else if (arg === "--help" || arg === "-h" || !arg) {
  help();
} else {
  console.error(`Unknown command: ${arg}`);
  help();
  process.exit(1);
}

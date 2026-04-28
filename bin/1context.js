#!/usr/bin/env node

const pkg = require("../package.json");

const arg = process.argv[2];

function main() {
  console.log(`1Context ${pkg.version}
Public bootstrap. Runtime coming soon.
https://github.com/hapticasensorics/1context`);
}

function help() {
  console.log(`1Context

Usage:
  1context
  1context --version
  1context --help
`);
}

if (arg === "--version" || arg === "-v" || arg === "version") {
  console.log(pkg.version);
} else if (arg === "--help" || arg === "-h" || !arg) {
  if (arg) {
    help();
  } else {
    main();
  }
} else {
  console.error(`Unknown command: ${arg}`);
  help();
  process.exit(1);
}

#!/usr/bin/env node

const fs = require("fs");
const http = require("http");
const https = require("https");
const os = require("os");
const path = require("path");

const pkg = require("../package.json");

const arg = process.argv[2];
const UPDATE_URL =
  process.env.ONECONTEXT_UPDATE_URL ||
  "https://api.github.com/repos/hapticasensorics/1context/releases/latest";
const HOMEBREW_UPDATE_COMMAND = "brew upgrade hapticasensorics/tap/1context";
const CHECK_INTERVAL_MS = Number(
  process.env.ONECONTEXT_UPDATE_CHECK_INTERVAL_MS || 24 * 60 * 60 * 1000,
);
const UPDATE_STATE_DIR =
  process.env.ONECONTEXT_UPDATE_STATE_DIR ||
  path.join(os.homedir(), ".config", "1context");
const UPDATE_STATE_PATH = path.join(UPDATE_STATE_DIR, "update-check.json");

function readUpdateState() {
  try {
    return JSON.parse(fs.readFileSync(UPDATE_STATE_PATH, "utf8"));
  } catch {
    return {};
  }
}

function writeUpdateState(state) {
  try {
    fs.mkdirSync(UPDATE_STATE_DIR, { recursive: true });
    fs.writeFileSync(UPDATE_STATE_PATH, `${JSON.stringify(state, null, 2)}\n`);
  } catch {
    // Update checks are best-effort and should never affect the CLI.
  }
}

function compareVersions(a, b) {
  const left = String(a || "")
    .replace(/^v/, "")
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const right = String(b || "")
    .replace(/^v/, "")
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const length = Math.max(left.length, right.length);

  for (let index = 0; index < length; index += 1) {
    const diff = (left[index] || 0) - (right[index] || 0);
    if (diff !== 0) return diff;
  }

  return 0;
}

function normalizeReleasePayload(payload) {
  const release = payload && payload.stable ? payload.stable : payload;
  const rawVersion = release && (release.version || release.tag_name || release.name);
  const version = String(rawVersion || "").replace(/^v/, "");

  return {
    version,
    notesUrl: release && (release.notes_url || release.html_url),
    install:
      release && release.install && release.install.homebrew
        ? release.install.homebrew
        : HOMEBREW_UPDATE_COMMAND,
  };
}

function fetchReleaseJson(url, redirectCount = 0) {
  return new Promise((resolve, reject) => {
    const parsedUrl = new URL(url);
    const client = parsedUrl.protocol === "http:" ? http : https;
    const request = client.get(
      parsedUrl,
      {
        headers: {
          accept: "application/json",
          "user-agent": `1context/${pkg.version}`,
        },
      },
      (response) => {
        const location = response.headers.location;
        if (
          location &&
          response.statusCode >= 300 &&
          response.statusCode < 400 &&
          redirectCount < 3
        ) {
          response.resume();
          resolve(
            fetchReleaseJson(new URL(location, url).toString(), redirectCount + 1),
          );
          return;
        }

        if (response.statusCode < 200 || response.statusCode >= 300) {
          response.resume();
          reject(new Error(`Update check returned ${response.statusCode}`));
          return;
        }

        let body = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          body += chunk;
          if (body.length > 64 * 1024) {
            request.destroy(new Error("Update response is too large"));
          }
        });
        response.on("end", () => {
          try {
            resolve(JSON.parse(body));
          } catch (error) {
            reject(error);
          }
        });
      },
    );

    request.setTimeout(750, () => {
      request.destroy(new Error("Update check timed out"));
    });
    request.on("error", reject);
  });
}

async function maybeCheckForUpdate() {
  if (process.env.ONECONTEXT_NO_UPDATE_CHECK === "1") return;

  const state = readUpdateState();
  const lastCheckedAt = Date.parse(state.last_checked_at || "");

  if (
    Number.isFinite(lastCheckedAt) &&
    Date.now() - lastCheckedAt < CHECK_INTERVAL_MS
  ) {
    return;
  }

  try {
    const latest = normalizeReleasePayload(await fetchReleaseJson(UPDATE_URL));
    if (!latest.version) return;

    writeUpdateState({
      last_checked_at: new Date().toISOString(),
      last_seen_latest: latest.version,
    });

    if (compareVersions(latest.version, pkg.version) > 0) {
      console.error(
        `1Context ${latest.version} is available. You have ${pkg.version}.`,
      );
      console.error(`Update: ${latest.install}`);
    }
  } catch {
    // Network and parsing failures stay silent.
  }
}

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

async function run() {
  if (arg === "--version" || arg === "-v" || arg === "version") {
    console.log(pkg.version);
  } else if (arg === "--help" || arg === "-h" || !arg) {
    if (arg) {
      help();
    } else {
      main();
      await maybeCheckForUpdate();
    }
  } else {
    console.error(`Unknown command: ${arg}`);
    help();
    process.exit(1);
  }
}

run().catch(() => {
  process.exit(1);
});

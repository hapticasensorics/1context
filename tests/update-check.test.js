const assert = require("assert");
const { spawn } = require("child_process");
const http = require("http");
const fs = require("fs");
const os = require("os");
const path = require("path");

const repoRoot = path.resolve(__dirname, "..");
const cliPath = path.join(repoRoot, "bin", "1context.js");

function startServer(payload) {
  const server = http.createServer((request, response) => {
    assert.strictEqual(request.url, "/repos/hapticasensorics/1context/releases/latest");
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify(payload));
  });

  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      resolve({
        close: () => new Promise((done) => server.close(done)),
        url: `http://127.0.0.1:${port}/repos/hapticasensorics/1context/releases/latest`,
      });
    });
  });
}

function runCli(args, env) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [cliPath, ...args], {
      cwd: repoRoot,
      env: { ...process.env, ...env },
    });
    let stdout = "";
    let stderr = "";

    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("close", (status) => {
      resolve({ status, stdout, stderr });
    });
  });
}

(async () => {
  const stateDir = fs.mkdtempSync(path.join(os.tmpdir(), "1context-update-"));
  const server = await startServer({
    tag_name: "v9.9.9",
    published_at: "2026-04-28T00:00:00Z",
    html_url: "https://github.com/hapticasensorics/1context/releases/tag/v9.9.9",
  });

  try {
    const env = {
      ONECONTEXT_UPDATE_URL: server.url,
      ONECONTEXT_UPDATE_STATE_DIR: stateDir,
      ONECONTEXT_UPDATE_CHECK_INTERVAL_MS: "0",
    };
    const result = await runCli([], env);

    assert.strictEqual(result.status, 0);
    assert.match(result.stdout, /1Context 0\.1\.2/);
    assert.match(
      result.stderr,
      /1Context 9\.9\.9 is available\. You have 0\.1\.2\./,
    );
    assert.match(
      result.stderr,
      /Update: brew upgrade hapticasensorics\/tap\/1context/,
    );

    const state = JSON.parse(
      fs.readFileSync(path.join(stateDir, "update-check.json"), "utf8"),
    );
    assert.strictEqual(state.last_seen_latest, "9.9.9");

    const versionResult = await runCli(["--version"], env);
    assert.strictEqual(versionResult.status, 0);
    assert.strictEqual(versionResult.stdout.trim(), "0.1.2");
    assert.strictEqual(versionResult.stderr, "");

    const disabledResult = await runCli([], {
      ...env,
      ONECONTEXT_NO_UPDATE_CHECK: "1",
    });
    assert.strictEqual(disabledResult.status, 0);
    assert.strictEqual(disabledResult.stderr, "");
  } finally {
    await server.close();
    fs.rmSync(stateDir, { recursive: true, force: true });
  }
})();

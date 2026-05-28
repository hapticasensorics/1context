import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { execa } from "execa";
import {
  checkCleanTree,
  envForManifest,
  loadManifestFile,
  matrixCases,
  validateAppcast,
  validateManifest,
  writeAssetManifest,
} from "../src/manifest.js";
import { fromRoot } from "../src/paths.js";

function tmpDir(name: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), `1ctx-${name}-`));
}

function writeAppcast(filePath: string, version: string, previousVersion: string, body = ""): void {
  fs.writeFileSync(filePath, `<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <item>
      <title>1Context ${version}</title>
      <sparkle:version>${version}</sparkle:version>
      <sparkle:minimumAutoupdateVersion>${previousVersion}</sparkle:minimumAutoupdateVersion>
      <sparkle:criticalUpdate sparkle:version="${version}"/>
${body}
      <enclosure url="https://github.com/hapticasensorics/1context/releases/download/v${version}/1Context-${version}-macos-arm64.dmg" length="12345" type="application/octet-stream" sparkle:edSignature="fixture-signature"/>
    </item>
  </channel>
</rss>
`);
}

test("manifest validates and exports dev channel policy", async () => {
  const manifest = await validateManifest({ channel: "dev" });
  const env = envForManifest(manifest, fromRoot("release", "release.toml"), "dev");
  assert.equal(env.ONECONTEXT_RELEASE_CHANNEL, "dev");
  assert.equal(env.ONECONTEXT_RELEASE_BUDGET_ADVISORY, "1");
  assert.equal(env.ONECONTEXT_RELEASE_CHANNEL_APPCAST, "none");
  assert.equal(env.ONECONTEXT_RELEASE_CHANNEL_SIGNING_MODE, "apple-development");
  assert.equal(env.ONECONTEXT_APP_IDENTITY, "dev");
  assert.equal(env.ONECONTEXT_APP_BUNDLE_NAME, "1Context Dev");
  assert.equal(env.ONECONTEXT_BUNDLE_IDENTIFIER, "com.haptica.1context.dev");
});

test("appcast policy accepts mandatory official appcast and rejects hidden release notes", () => {
  const manifest = loadManifestFile();
  const dir = tmpDir("appcast");
  const ok = path.join(dir, "ok.xml");
  const withNotes = path.join(dir, "with-notes.xml");
  writeAppcast(ok, manifest.version, manifest.previous_version);
  writeAppcast(withNotes, manifest.version, manifest.previous_version, "      <description>Builder notes should stay hidden.</description>");
  validateAppcast(manifest, ok, "official");
  assert.throws(() => validateAppcast(manifest, withNotes, "official"), /hides updater release notes/);
});

test("asset manifest stores release-relative paths", async () => {
  const manifest = loadManifestFile();
  const dir = tmpDir("assets");
  for (const name of [
    `1Context-${manifest.version}-macos-arm64.dmg`,
    `1Context-${manifest.version}-macos-arm64.dmg.sha256`,
    "1Context.dmg",
    "1Context.dmg.sha256",
    "appcast.xml",
  ]) {
    fs.writeFileSync(path.join(dir, name), `${name}\n`);
  }
  const output = path.join(dir, "asset-manifest.json");
  await writeAssetManifest(manifest, dir, output);
  const payload = JSON.parse(fs.readFileSync(output, "utf8")) as { assets: Array<{ path: string }> };
  assert.equal(payload.assets.length, 5);
  assert(payload.assets.every((asset) => asset.path.startsWith("dist/")));
  assert(!JSON.stringify(payload).includes("/Users/"));
});

test("updater matrix only lists real proof cases", () => {
  const manifest = loadManifestFile();
  assert.deepEqual(matrixCases(manifest), [
    "already_current_manual_check",
    "mandatory_automatic_success",
    "stale_sparkle_defaults",
    "old_app_with_new_appcast",
    "app_relaunch_recovery",
    "login_restart_recovery",
    "real_uninstall_reinstall",
  ]);
  assert(
    manifest.updater_matrix.every((item) =>
      new Set(["self_hosted_gui_update", "real_uninstall_reinstall"]).has(item.proof)
    )
  );
});

test("clean-tree gate rejects untracked files", async () => {
  const dir = tmpDir("dirty-repo");
  await execa("git", ["-C", dir, "init", "-q"]);
  fs.writeFileSync(path.join(dir, "tracked.txt"), "clean\n");
  await execa("git", ["-C", dir, "add", "tracked.txt"]);
  await execa("git", ["-C", dir, "-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false", "commit", "-qm", "init"]);
  fs.writeFileSync(path.join(dir, "untracked.txt"), "dirty\n");
  await assert.rejects(() => checkCleanTree(dir), /Release tree is dirty/);
});

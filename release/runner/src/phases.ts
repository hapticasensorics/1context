import fs from "node:fs";
import crypto from "node:crypto";
import os from "node:os";
import path from "node:path";
import { execa } from "execa";
import { ReleaseError, assertRelease } from "./errors.js";
import { fromRoot, repoRoot } from "./paths.js";
import type { ReleaseContext } from "./context.js";
import {
  matrixCases,
  validateAppcast,
  validateManifest,
  writeAssetManifest,
  writeFixtureProofResults,
} from "./manifest.js";
import { ensureEvidenceDirs, timeReleaseStep, writeJson, writeReleaseEvidence, writeStageTiming } from "./evidence.js";
import { requireTool, runCapture, runCommand } from "./exec.js";

function ctxEnv(ctx: ReleaseContext, extra: Record<string, string> = {}): NodeJS.ProcessEnv {
  return {
    ...process.env,
    ...ctx.env,
    ...extra,
  };
}

function fail(message: string): never {
  throw new ReleaseError(message);
}

function quoteCommand(args: string[]): string {
  return args.map((value) => {
    if (/^[A-Za-z0-9_./:@%+=,-]+$/.test(value)) return value;
    return `'${value.replaceAll("'", "'\"'\"'")}'`;
  }).join(" ") + "\n";
}

async function ensureTagRef(ctx: ReleaseContext): Promise<void> {
  if (process.env.GITHUB_REF) {
    assertRelease(process.env.GITHUB_REF === `refs/tags/${ctx.tag}`, `Release must run from ${ctx.tag}; current ref is ${process.env.GITHUB_REF}.`);
    return;
  }
  const result = await execa("git", ["-C", ctx.root, "describe", "--tags", "--exact-match"], { reject: false });
  const currentTag = result.exitCode === 0 ? result.stdout.trim() : "";
  assertRelease(currentTag === ctx.tag, `Release must run from tag ${ctx.tag}; current checkout is ${currentTag || "not exactly tagged"}.`);
}

async function releaseValidate(ctx: ReleaseContext): Promise<void> {
  await validateManifest({
    channel: ctx.channel,
    requireClean: ctx.policy.requires_clean_tree,
  });
  if (ctx.policy.requires_tag) {
    await ensureTagRef(ctx);
  }
}

async function developerIdIdentity(ctx: ReleaseContext): Promise<string> {
  const keychain = process.env.CODESIGN_KEYCHAIN || process.env.ONECONTEXT_RELEASE_KEYCHAIN || "";
  const args = ["find-identity", "-v", "-p", "codesigning"];
  if (keychain) args.push(keychain);
  const output = await runCapture("security", args, { env: ctxEnv(ctx) });
  const match = /"([^"]*Developer ID Application:[^"]*)"/.exec(output);
  return match?.[1] ?? "";
}

async function sparklePublicKey(ctx: ReleaseContext): Promise<string> {
  const generateKeys = fromRoot("macos", ".build", "artifacts", "sparkle", "Sparkle", "bin", "generate_keys");
  if (!fs.existsSync(generateKeys)) {
    await runCommand("swift", ["build", "--package-path", fromRoot("macos"), "-c", "release"], { env: ctxEnv(ctx), stdout: "ignore" });
  }
  const account = process.env.SPARKLE_KEY_ACCOUNT ?? "com.haptica.1context.sparkle";
  const output = await runCapture(generateKeys, ["--account", account, "-p"], { env: ctxEnv(ctx) });
  const xmlMatch = /<string>([^<]+)<\/string>/.exec(output);
  if (xmlMatch?.[1]) return xmlMatch[1].trim();
  const line = output.split(/\r?\n/).find((item) => /^[A-Za-z0-9+/]+=*$/.test(item.trim()));
  return line?.trim() ?? "";
}

async function writeChecksumsForReleaseAssets(ctx: ReleaseContext): Promise<void> {
  await execa("bash", ["-c", `cd "$1" && shasum -a 256 "$2" > "$2.sha256" && shasum -a 256 "$3" > "$3.sha256"`, "bash", "dist", `1Context-${ctx.version}-macos-arm64.dmg`, ctx.stableDmgName], {
    cwd: ctx.root,
    stdio: "inherit",
    env: ctxEnv(ctx),
  });
}

async function collectOfficialReleaseAssets(ctx: ReleaseContext): Promise<void> {
  const artifact = fromRoot("dist", `1Context-${ctx.version}-macos-arm64.dmg`);
  const appcast = fromRoot("dist", "sparkle-updates", "appcast.xml");
  assertRelease(fs.existsSync(artifact), `Missing versioned DMG: ${artifact}`);
  assertRelease(fs.existsSync(appcast), `Missing generated appcast: ${appcast}`);
  fs.copyFileSync(appcast, fromRoot("dist", "appcast.xml"));
  fs.copyFileSync(artifact, fromRoot("dist", ctx.stableDmgName));
  await writeChecksumsForReleaseAssets(ctx);
  validateAppcast(ctx.manifest, fromRoot("dist", "appcast.xml"), "official");
  await writeAssetManifest(ctx.manifest, fromRoot("dist"), ctx.assetManifest);
}

async function writeSparkleFixtureProofResults(ctx: ReleaseContext): Promise<void> {
  const testLog = path.join(ctx.evidenceDir, "sparkle-fixture-tests.log");
  await timeReleaseStep(ctx, "prove", "run_sparkle_fixture_tests", async () => {
    await execa("bash", ["-c", `swift test --package-path "$1" --filter OneContextSparkleUpdateTests > "$2" 2>&1`, "bash", fromRoot("macos"), testLog], {
      cwd: ctx.root,
      stdio: "inherit",
      env: ctxEnv(ctx),
    });
  });
  await timeReleaseStep(ctx, "prove", "write_sparkle_fixture_results", async () => {
    writeFixtureProofResults(ctx.manifest, ctx.proofResultsDir);
  });
}

function collectDownloadedProofResults(ctx: ReleaseContext, artifactDir: string): void {
  fs.mkdirSync(ctx.proofResultsDir, { recursive: true });
  const matches: string[] = [];
  const walk = (dir: string): void => {
    for (const name of fs.readdirSync(dir)) {
      const item = path.join(dir, name);
      const stat = fs.statSync(item);
      if (stat.isDirectory()) {
        walk(item);
      } else if (item.includes(`${path.sep}proof-results${path.sep}`) && item.endsWith(".json")) {
        matches.push(item);
      }
    }
  };
  walk(artifactDir);
  matches.sort();
  fs.writeFileSync(path.join(ctx.evidenceDir, "downloaded-proof-results.txt"), matches.join("\n") + (matches.length ? "\n" : ""));
  assertRelease(matches.length > 0, `Downloaded self-hosted proof artifacts did not contain proof-results/*.json under ${artifactDir}.`);
  for (const proofJson of matches) {
    fs.copyFileSync(proofJson, path.join(ctx.proofResultsDir, path.basename(proofJson)));
  }
}

export async function phaseValidate(ctx: ReleaseContext): Promise<void> {
  const startedMs = Date.now();
  try {
    await releaseValidate(ctx);
    writeStageTiming(ctx, "validate", "passed", startedMs);
  } catch (error) {
    writeStageTiming(ctx, "validate", "failed", startedMs);
    throw error;
  }
}

export async function phaseBuild(ctx: ReleaseContext, args: string[]): Promise<void> {
  let dryRun = false;
  while (args.length > 0) {
    const arg = args.shift();
    if (arg === "--dry-run") {
      dryRun = true;
    } else {
      fail(`Unknown build argument: ${arg}`);
    }
  }
  const startedMs = Date.now();
  await timeReleaseStep(ctx, "build", "validate_preflight", () => releaseValidate(ctx));
  ensureEvidenceDirs(ctx);
  await timeReleaseStep(ctx, "build", "write_runner_attestation", () => runCommand(fromRoot("release", "tools", "write-runner-attestation.sh"), [ctx.runnerAttestation], { env: ctxEnv(ctx) }));
  if (dryRun) {
    writeReleaseEvidence(ctx, `build-${ctx.channel}-dry-run`);
    writeStageTiming(ctx, "build", "dry-run", startedMs);
    return;
  }

  const buildEnv: Record<string, string> = {
    ONECONTEXT_VERSION: ctx.version,
    ONECONTEXT_SIGNING_MODE: ctx.policy.signing_mode,
  };
  if (ctx.policy.signing_mode === "developer-id") {
    buildEnv.CODESIGN_IDENTITY = process.env.CODESIGN_IDENTITY || await developerIdIdentity(ctx);
    assertRelease(buildEnv.CODESIGN_IDENTITY.length > 0, "No Developer ID Application signing identity found.");
  }
  if (ctx.policy.appcast === "none") {
    buildEnv.ONECONTEXT_SPARKLE_PUBLIC_ED_KEY = "";
  } else {
    buildEnv.ONECONTEXT_SPARKLE_PUBLIC_ED_KEY = process.env.ONECONTEXT_SPARKLE_PUBLIC_ED_KEY || await sparklePublicKey(ctx);
    assertRelease(buildEnv.ONECONTEXT_SPARKLE_PUBLIC_ED_KEY.length > 0, `No Sparkle public key found for account '${process.env.SPARKLE_KEY_ACCOUNT ?? "com.haptica.1context.sparkle"}'. Create or restore the Sparkle EdDSA key in the release keychain.`);
  }

  await timeReleaseStep(ctx, "build", "build_app_bundle", () => runCommand(fromRoot("scripts", "build-macos-app.sh"), [], { env: ctxEnv(ctx, buildEnv) }));
  if (ctx.channel === "dev") {
    await timeReleaseStep(ctx, "build", "create_dmg", () => runCommand(fromRoot("release", "tools", "create-macos-dmg.sh"), [fromRoot("dist", "1Context.app"), ctx.dmg], { env: ctxEnv(ctx, buildEnv), stdout: "ignore" }));
    await timeReleaseStep(ctx, "build", "validate_dmg", () => runCommand(fromRoot("release", "tools", "validate-macos-dmg.sh"), [ctx.dmg], { env: ctxEnv(ctx, { ...buildEnv, ALLOW_UNNOTARIZED: "1" }) }));
    writeReleaseEvidence(ctx, `build-${ctx.channel}`);
    await timeReleaseStep(ctx, "build", "redact_evidence", () => runCommand(fromRoot("release", "tools", "redact-evidence.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx, buildEnv) }));
    await timeReleaseStep(ctx, "build", "audit_evidence_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx, buildEnv) }));
    writeStageTiming(ctx, "build", "passed", startedMs);
    return;
  }

  if (ctx.policy.notarize) {
    await timeReleaseStep(ctx, "build", "notarize_app_bundle", () => runCommand(fromRoot("release", "tools", "notarize-macos-artifact.sh"), [fromRoot("dist", "1Context.app")], { env: ctxEnv(ctx, buildEnv) }));
  }
  await timeReleaseStep(ctx, "build", "create_dmg", () => runCommand(fromRoot("release", "tools", "create-macos-dmg.sh"), [fromRoot("dist", "1Context.app"), ctx.dmg], { env: ctxEnv(ctx, buildEnv), stdout: "ignore" }));
  if (ctx.policy.signing_mode === "developer-id") {
    const codesignArgs = ["--force", "--timestamp", "--sign", buildEnv.CODESIGN_IDENTITY ?? ""];
    const keychain = process.env.CODESIGN_KEYCHAIN || process.env.ONECONTEXT_RELEASE_KEYCHAIN || "";
    if (keychain) codesignArgs.push("--keychain", keychain);
    await timeReleaseStep(ctx, "build", "sign_dmg", () => runCommand("codesign", [...codesignArgs, ctx.dmg], { env: ctxEnv(ctx, buildEnv), stdout: "ignore" }));
    await timeReleaseStep(ctx, "build", "verify_signed_dmg", () => runCommand("codesign", ["--verify", "--strict", ctx.dmg], { env: ctxEnv(ctx, buildEnv), stdout: "ignore" }));
  }
  if (ctx.policy.notarize) {
    await timeReleaseStep(ctx, "build", "notarize_dmg", () => runCommand(fromRoot("release", "tools", "notarize-macos-artifact.sh"), [ctx.dmg], { env: ctxEnv(ctx, buildEnv) }));
    await timeReleaseStep(ctx, "build", "validate_dmg", () => runCommand(fromRoot("release", "tools", "validate-macos-dmg.sh"), [ctx.dmg], { env: ctxEnv(ctx, buildEnv) }));
  } else {
    await timeReleaseStep(ctx, "build", "validate_dmg", () => runCommand(fromRoot("release", "tools", "validate-macos-dmg.sh"), [ctx.dmg], { env: ctxEnv(ctx, { ...buildEnv, ALLOW_UNNOTARIZED: "1" }) }));
  }
  if (ctx.policy.appcast !== "none") {
    await timeReleaseStep(ctx, "build", "generate_appcast", () => runCommand(fromRoot("release", "tools", "generate-sparkle-appcast.sh"), [ctx.dmg], { env: ctxEnv(ctx, buildEnv) }));
    await timeReleaseStep(ctx, "build", "validate_appcast", () => validateAppcast(ctx.manifest, fromRoot("dist", "sparkle-updates", "appcast.xml"), ctx.channel));
    if (ctx.channel === "official") {
      await timeReleaseStep(ctx, "build", "collect_release_assets", () => collectOfficialReleaseAssets(ctx));
    } else {
      fs.mkdirSync(fromRoot("dist", ctx.channel), { recursive: true });
      fs.copyFileSync(fromRoot("dist", "sparkle-updates", "appcast.xml"), fromRoot("dist", ctx.channel, "appcast.xml"));
    }
  }
  writeReleaseEvidence(ctx, `build-${ctx.channel}`);
  await timeReleaseStep(ctx, "build", "redact_evidence", () => runCommand(fromRoot("release", "tools", "redact-evidence.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx, buildEnv) }));
  await timeReleaseStep(ctx, "build", "audit_evidence_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx, buildEnv) }));
  writeStageTiming(ctx, "build", "passed", startedMs);
}

async function auditPublicReleaseAssets(ctx: ReleaseContext, tag: string): Promise<void> {
  const repo = process.env.ONECONTEXT_GITHUB_REPO ?? "hapticasensorics/1context";
  const probes = Number.parseInt(process.env.ONECONTEXT_RELEASE_AUDIT_PROBES ?? "1", 10);
  const interval = Number.parseInt(process.env.ONECONTEXT_RELEASE_AUDIT_INTERVAL_SECONDS ?? "0", 10);
  assertRelease(Number.isInteger(probes) && probes >= 1, "ONECONTEXT_RELEASE_AUDIT_PROBES must be a positive integer.");
  assertRelease(Number.isInteger(interval) && interval >= 0, "ONECONTEXT_RELEASE_AUDIT_INTERVAL_SECONDS must be a non-negative integer.");
  const latestAppcastUrl = process.env.ONECONTEXT_LATEST_APPCAST_URL ?? `https://github.com/${repo}/releases/latest/download/appcast.xml`;
  const stableDmgUrl = process.env.ONECONTEXT_STABLE_DMG_URL ?? `https://github.com/${repo}/releases/latest/download/1Context.dmg`;
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), "1context-release-audit-"));
  try {
    await requireTool("gh");
    await requireTool("curl");
    console.log(`[release-audit] reading GitHub release ${repo}@${tag}`);
    const releaseJson = await runCapture("gh", ["release", "view", tag, "--repo", repo, "--json", "tagName,isDraft,isPrerelease,assets,url"], { env: ctxEnv(ctx) });
    const release = JSON.parse(releaseJson) as { tagName?: string; isDraft?: boolean; isPrerelease?: boolean; assets?: Array<{ name: string }>; url?: string };
    assertRelease(release.tagName === tag, `release tag ${JSON.stringify(release.tagName)} != expected ${JSON.stringify(tag)}`);
    assertRelease(!release.isDraft, "release is still draft");
    assertRelease(!release.isPrerelease, "release is marked prerelease");
    const assets = new Set((release.assets ?? []).map((asset) => asset.name));
    const required = new Set([
      `1Context-${ctx.version}-macos-arm64.dmg`,
      `1Context-${ctx.version}-macos-arm64.dmg.sha256`,
      "1Context.dmg",
      "1Context.dmg.sha256",
      "appcast.xml",
      "asset-manifest.json",
    ]);
    const missing = [...required].filter((asset) => !assets.has(asset));
    assertRelease(missing.length === 0, `release is missing assets: ${missing.join(", ")}`);
    if (release.url) console.log(release.url);

    console.log("[release-audit] downloading appcast.xml");
    await runCommand("gh", ["release", "download", tag, "--repo", repo, "--pattern", "appcast.xml", "--dir", workDir, "--clobber"], { env: ctxEnv(ctx), stdout: "ignore" });
    validateAppcast(ctx.manifest, path.join(workDir, "appcast.xml"), ctx.channel);
    await runCommand("gh", ["release", "download", tag, "--repo", repo, "--pattern", "asset-manifest.json", "--dir", workDir, "--clobber"], { env: ctxEnv(ctx), stdout: "ignore" });
    fs.mkdirSync(path.dirname(ctx.assetManifest), { recursive: true });
    fs.copyFileSync(path.join(workDir, "asset-manifest.json"), ctx.assetManifest);

    let latestOk = false;
    console.log("[release-audit] checking latest/download appcast propagation");
    for (let probe = 1; probe <= probes; probe += 1) {
      if (probe > 1 && interval > 0) await new Promise((resolve) => setTimeout(resolve, interval * 1000));
      const latestAppcast = path.join(workDir, "latest-appcast.xml");
      const curl = await execa("curl", ["--fail", "--location", "--silent", "--show-error", latestAppcastUrl, "--output", latestAppcast], { reject: false });
      if (curl.exitCode === 0) {
        try {
          validateAppcast(ctx.manifest, latestAppcast, ctx.channel);
          if (fs.readFileSync(path.join(workDir, "appcast.xml")).equals(fs.readFileSync(latestAppcast))) {
            latestOk = true;
            console.log(`[release-audit] latest/download appcast probe ${probe}/${probes} passed`);
            break;
          }
        } catch {
          // Report through the retry loop below.
        }
      }
      console.log(`[release-audit] latest/download appcast probe ${probe}/${probes} has not propagated yet`);
    }
    assertRelease(latestOk, `latest/download appcast does not match the ${tag} appcast yet: ${latestAppcastUrl}`);

    await runCommand("gh", ["release", "download", tag, "--repo", repo, "--pattern", `1Context-${ctx.version}-macos-arm64.dmg.sha256`, "--pattern", "1Context.dmg.sha256", "--dir", workDir, "--clobber"], { env: ctxEnv(ctx), stdout: "ignore" });
    const appcastText = fs.readFileSync(path.join(workDir, "appcast.xml"), "utf8");
    const enclosureUrl = /<enclosure[^>]+url="([^"]+)"/.exec(appcastText)?.[1] ?? "";
    const enclosureLength = /<enclosure[^>]+length="([^"]+)"/.exec(appcastText)?.[1] ?? "";
    const enclosureName = path.basename(new URL(enclosureUrl).pathname);
    assertRelease(enclosureName === `1Context-${ctx.version}-macos-arm64.dmg`, `appcast enclosure asset ${JSON.stringify(enclosureName)} != expected 1Context-${ctx.version}-macos-arm64.dmg`);
    const expectedSha = fs.readFileSync(path.join(workDir, `${enclosureName}.sha256`), "utf8").trim().split(/\s+/)[0] ?? "";
    const stableExpectedSha = fs.readFileSync(path.join(workDir, "1Context.dmg.sha256"), "utf8").trim().split(/\s+/)[0] ?? "";
    assertRelease(expectedSha.length > 0, `Release checksum file for ${enclosureName} is empty.`);
    assertRelease(stableExpectedSha.length > 0, "Release checksum file for 1Context.dmg is empty.");
    assertRelease(stableExpectedSha === expectedSha, `Stable 1Context.dmg checksum ${stableExpectedSha} != versioned ${enclosureName} checksum ${expectedSha}.`);

    for (let probe = 1; probe <= probes; probe += 1) {
      if (probe > 1 && interval > 0) await new Promise((resolve) => setTimeout(resolve, interval * 1000));
      const output = path.join(workDir, `enclosure-${probe}.dmg`);
      await runCommand("curl", ["--fail", "--location", "--silent", "--show-error", enclosureUrl, "--output", output], { env: ctxEnv(ctx) });
      const actualSize = String(fs.statSync(output).size);
      assertRelease(!enclosureLength || actualSize === enclosureLength, `Downloaded enclosure size ${actualSize} != appcast length ${enclosureLength} on probe ${probe}.`);
      const actualSha = (await runCapture("shasum", ["-a", "256", output], { env: ctxEnv(ctx) })).trim().split(/\s+/)[0] ?? "";
      assertRelease(actualSha === expectedSha, `Downloaded enclosure sha256 ${actualSha} != expected ${expectedSha} on probe ${probe}.`);
      console.log(`[release-audit] appcast enclosure probe ${probe}/${probes} passed`);

      const stableOutput = path.join(workDir, `stable-${probe}.dmg`);
      await runCommand("curl", ["--fail", "--location", "--silent", "--show-error", stableDmgUrl, "--output", stableOutput], { env: ctxEnv(ctx) });
      const stableSize = String(fs.statSync(stableOutput).size);
      assertRelease(!enclosureLength || stableSize === enclosureLength, `Downloaded stable 1Context.dmg size ${stableSize} != appcast length ${enclosureLength} on probe ${probe}.`);
      const stableSha = (await runCapture("shasum", ["-a", "256", stableOutput], { env: ctxEnv(ctx) })).trim().split(/\s+/)[0] ?? "";
      assertRelease(stableSha === expectedSha, `Downloaded stable 1Context.dmg sha256 ${stableSha} != versioned expected ${expectedSha} on probe ${probe}.`);
      console.log(`[release-audit] stable 1Context.dmg probe ${probe}/${probes} passed`);
    }
    console.log(`[release-audit] release asset audit passed for ${tag}`);
  } finally {
    fs.rmSync(workDir, { recursive: true, force: true });
  }
}

export async function phasePublish(ctx: ReleaseContext): Promise<void> {
  const startedMs = Date.now();
  if (ctx.channel === "private") {
    await phasePublishPrivate(ctx, startedMs);
    return;
  }
  if (ctx.channel !== "official") {
    fail(`publish --channel ${ctx.channel} is not wired yet; only official public publishing is active.`);
  }
  await requireTool("gh");
  await timeReleaseStep(ctx, "publish", "validate_preflight", () => releaseValidate(ctx));
  await timeReleaseStep(ctx, "publish", "ensure_tag_ref", () => ensureTagRef(ctx));
  ensureEvidenceDirs(ctx);
  if (!fs.existsSync(ctx.assetManifest)) {
    await timeReleaseStep(ctx, "publish", "write_asset_manifest", () => writeAssetManifest(ctx.manifest, fromRoot("dist"), ctx.assetManifest));
  }
  await timeReleaseStep(ctx, "publish", "write_runner_attestation", () => runCommand(fromRoot("release", "tools", "write-runner-attestation.sh"), [ctx.runnerAttestation], { env: ctxEnv(ctx) }));
  writeReleaseEvidence(ctx, "publish-preflight");
  await timeReleaseStep(ctx, "publish", "redact_preflight_evidence", () => runCommand(fromRoot("release", "tools", "redact-evidence.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  await timeReleaseStep(ctx, "publish", "audit_preflight_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  const view = await execa("gh", ["release", "view", ctx.tag], { cwd: ctx.root, env: ctxEnv(ctx), reject: false });
  if (view.exitCode !== 0) {
    await timeReleaseStep(ctx, "publish", "create_github_release", () => runCommand("gh", ["release", "create", ctx.tag, "--title", `1Context ${ctx.tag}`, "--notes-file", fromRoot("RELEASE_NOTES.md")], { env: ctxEnv(ctx) }));
  }
  await timeReleaseStep(ctx, "publish", "upload_official_assets", () => runCommand("gh", [
    "release", "upload", ctx.tag,
    fromRoot("dist", `1Context-${ctx.version}-macos-arm64.dmg`),
    fromRoot("dist", `1Context-${ctx.version}-macos-arm64.dmg.sha256`),
    fromRoot("dist", ctx.stableDmgName),
    fromRoot("dist", `${ctx.stableDmgName}.sha256`),
    fromRoot("dist", "appcast.xml"),
    ctx.assetManifest,
    "--clobber",
  ], { env: ctxEnv(ctx) }));
  await timeReleaseStep(ctx, "publish", "audit_public_release_assets", () => auditPublicReleaseAssets(ctx, ctx.tag));
  writeReleaseEvidence(ctx, "publish");
  await timeReleaseStep(ctx, "publish", "redact_evidence", () => runCommand(fromRoot("release", "tools", "redact-evidence.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  await timeReleaseStep(ctx, "publish", "audit_evidence_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  writeStageTiming(ctx, "publish", "passed", startedMs);
}

async function phasePublishPrivate(ctx: ReleaseContext, startedMs: number): Promise<void> {
  const repo = ctx.policy.artifact_repo ?? "";
  const privateDir = fromRoot("dist", "private");
  const privateAppcast = path.join(privateDir, "appcast.xml");
  const privateAssetManifest = path.join(ctx.evidenceDir, "private-asset-manifest.json");
  const versionedDmg = fromRoot("dist", `1Context-${ctx.version}-macos-${ctx.arch}.dmg`);
  const versionedSha = path.join(privateDir, `1Context-${ctx.version}-macos-${ctx.arch}.dmg.sha256`);
  await requireTool("gh");
  await timeReleaseStep(ctx, "publish", "validate_preflight", () => releaseValidate(ctx));
  fs.mkdirSync(ctx.evidenceDir, { recursive: true });
  fs.mkdirSync(privateDir, { recursive: true });
  assertRelease(fs.existsSync(versionedDmg), `Missing private release DMG: ${versionedDmg}`);
  assertRelease(fs.existsSync(privateAppcast), "Missing private appcast. Run: scripts/release-train.sh build --channel private");
  await timeReleaseStep(ctx, "publish", "validate_private_appcast", () => validateAppcast(ctx.manifest, privateAppcast, "private"));
  await timeReleaseStep(ctx, "publish", "write_private_checksum", () => execa("bash", ["-c", `shasum -a 256 "$1" > "$2"`, "bash", versionedDmg, versionedSha], { cwd: ctx.root, stdio: "inherit", env: ctxEnv(ctx) }));
  await timeReleaseStep(ctx, "publish", "write_runner_attestation", () => runCommand(fromRoot("release", "tools", "write-runner-attestation.sh"), [ctx.runnerAttestation], { env: ctxEnv(ctx) }));
  const assets = [versionedDmg, versionedSha, privateAppcast].map((assetPath) => {
    const buffer = fs.readFileSync(assetPath);
    return {
      name: path.basename(assetPath),
      path: assetPath,
      size: buffer.byteLength,
      sha256: crypto.createHash("sha256").update(buffer).digest("hex"),
    };
  });
  writeJson(privateAssetManifest, {
    schema_version: "1context.private-asset-manifest.v1",
    version: ctx.version,
    tag: ctx.tag,
    repo,
    generated_at: new Date().toISOString(),
    assets,
  });
  const view = await execa("gh", ["release", "view", ctx.tag, "--repo", repo], { cwd: ctx.root, env: ctxEnv(ctx), reject: false });
  if (view.exitCode !== 0) {
    await timeReleaseStep(ctx, "publish", "create_private_release", () => runCommand("gh", ["release", "create", ctx.tag, "--repo", repo, "--target", "main", "--title", `1Context ${ctx.tag} private`, "--notes", `Private 1Context release-factory update assets for ${ctx.tag}.`], { env: ctxEnv(ctx) }));
  }
  await timeReleaseStep(ctx, "publish", "upload_private_assets", () => runCommand("gh", [
    "release", "upload", ctx.tag, "--repo", repo,
    versionedDmg,
    versionedSha,
    `${privateAppcast}#appcast.xml`,
    privateAssetManifest,
    "--clobber",
  ], { env: ctxEnv(ctx) }));
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), "1context-private-release-audit-"));
  try {
    await timeReleaseStep(ctx, "publish", "download_private_appcast", () => runCommand("gh", ["release", "download", ctx.tag, "--repo", repo, "--pattern", "appcast.xml", "--dir", workDir, "--clobber"], { env: ctxEnv(ctx), stdout: "ignore" }));
    await timeReleaseStep(ctx, "publish", "download_private_dmg", () => runCommand("gh", ["release", "download", ctx.tag, "--repo", repo, "--pattern", `1Context-${ctx.version}-macos-${ctx.arch}.dmg`, "--dir", workDir, "--clobber"], { env: ctxEnv(ctx), stdout: "ignore" }));
    await timeReleaseStep(ctx, "publish", "download_private_checksum", () => runCommand("gh", ["release", "download", ctx.tag, "--repo", repo, "--pattern", `1Context-${ctx.version}-macos-${ctx.arch}.dmg.sha256`, "--dir", workDir, "--clobber"], { env: ctxEnv(ctx), stdout: "ignore" }));
    await timeReleaseStep(ctx, "publish", "audit_private_appcast", () => validateAppcast(ctx.manifest, path.join(workDir, "appcast.xml"), "private"));
    await timeReleaseStep(ctx, "publish", "audit_private_checksum", () => execa("bash", ["-c", `cd "$1" && shasum -a 256 --check "$2" >/dev/null`, "bash", workDir, `1Context-${ctx.version}-macos-${ctx.arch}.dmg.sha256`], { cwd: ctx.root, stdio: "inherit", env: ctxEnv(ctx) }));
  } finally {
    fs.rmSync(workDir, { recursive: true, force: true });
  }
  writeReleaseEvidence(ctx, "publish-private");
  await timeReleaseStep(ctx, "publish", "redact_evidence", () => runCommand(fromRoot("release", "tools", "redact-evidence.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  await timeReleaseStep(ctx, "publish", "audit_evidence_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  writeStageTiming(ctx, "publish", "passed", startedMs);
}

export async function phaseProve(ctx: ReleaseContext, args: string[]): Promise<void> {
  const startedMs = Date.now();
  let mode = "dispatch";
  let repo = process.env.ONECONTEXT_GITHUB_REPO ?? "hapticasensorics/1context";
  let defaultWorkflow = "self-hosted-mac-update-proof.yml";
  let ref = ctx.tag;
  let proofReason = `manifest-driven ${ctx.updateClass} Sparkle proof for 1Context ${ctx.version}`;
  if (ctx.channel === "private") {
    defaultWorkflow = "self-hosted-mac-private-update-proof.yml";
    ref = "main";
  }
  let workflow = process.env.ONECONTEXT_RELEASE_PROOF_WORKFLOW ?? defaultWorkflow;
  while (args.length > 0) {
    const arg = args.shift();
    if (arg === "--dry-run") mode = "dry-run";
    else if (arg === "--dispatch") mode = "dispatch";
    else if (arg === "--runner-execute") mode = "runner-execute";
    else if (arg === "--repo") repo = args.shift() ?? fail("Missing --repo value");
    else if (arg === "--workflow") workflow = args.shift() ?? fail("Missing --workflow value");
    else if (arg === "--ref") ref = args.shift() ?? fail("Missing --ref value");
    else if (arg === "--proof-reason") proofReason = args.shift() ?? fail("Missing --proof-reason value");
    else fail(`Unknown prove argument: ${arg}`);
  }

  if (mode === "runner-execute") {
    await validateManifest({ channel: ctx.channel, requireClean: true });
    for (const name of [
      "ONECONTEXT_OLD_VERSION",
      "ONECONTEXT_NEW_VERSION",
      "ONECONTEXT_OLD_TAG",
      "ONECONTEXT_OLD_DMG_URL",
      "ONECONTEXT_STAGING_APPCAST_URL",
      "ONECONTEXT_EXPECTED_UPDATE_CLASS",
      "ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS",
      "ONECONTEXT_STEADY_STATE_SECONDS",
    ]) {
      assertRelease(!process.env[name], `Runner release facts must come from release/release.toml; unset ${name}.`);
    }
    const env: Record<string, string> = {
      ONECONTEXT_OLD_VERSION: ctx.previousVersion,
      ONECONTEXT_NEW_VERSION: ctx.version,
      ONECONTEXT_STAGING_APPCAST_URL: ctx.env.ONECONTEXT_SPARKLE_FEED_URL ?? "",
      ONECONTEXT_EXPECTED_UPDATE_CLASS: ctx.updateClass,
      ONECONTEXT_REMOTE_UPDATE_MANIFEST_CHANNEL: ctx.channel,
    };
    if (ctx.channel === "private") {
      env.ONECONTEXT_RUN_UNINSTALL_REINSTALL_PROOF = "0";
      env.ONECONTEXT_GITHUB_REPO = ctx.policy.artifact_repo ?? "";
      env.ONECONTEXT_REMOTE_APPCAST_GITHUB_REPO = env.ONECONTEXT_GITHUB_REPO;
      env.ONECONTEXT_UPDATE_RUNNER_ALLOW_NON_PUBLIC_FINAL_FEED = "1";
      env.ONECONTEXT_UPDATE_RUNNER_RESTORE_PUBLIC_FINAL_FEED = "0";
    } else {
      env.ONECONTEXT_RUN_UNINSTALL_REINSTALL_PROOF = "1";
      env.ONECONTEXT_UPDATE_RUNNER_ALLOW_DELETE_DATA = "1";
    }
    await runCommand(fromRoot("release", "tools", "proof", "self-hosted-update-proof.sh"), [], { env: ctxEnv(ctx, env) });
    return;
  }

  assertRelease(/^(main|release\/.*|rc\/.*|v.*)$/.test(ref), `Ref '${ref}' is not allowed for the self-hosted runner. Use main, release/*, rc/*, or a v* tag.`);
  if (mode !== "dry-run") {
    await requireTool("gh");
    await timeReleaseStep(ctx, "prove", "validate_preflight", () => releaseValidate(ctx));
    if (ctx.policy.requires_tag) {
      await timeReleaseStep(ctx, "prove", "ensure_tag_ref", () => ensureTagRef(ctx));
    }
  } else {
    await timeReleaseStep(ctx, "prove", "validate_preflight", () => validateManifest({ channel: ctx.channel }));
  }
  ensureEvidenceDirs(ctx);
  await timeReleaseStep(ctx, "prove", "write_runner_attestation", () => runCommand(fromRoot("release", "tools", "write-runner-attestation.sh"), [ctx.runnerAttestation], { env: ctxEnv(ctx) }));
  const transcript = path.join(ctx.evidenceDir, "release-proof-request.txt");
  const cmd = [
    "gh", "workflow", "run", workflow,
    "--repo", repo,
    "--ref", ref,
    "-f", `proof_reason=${proofReason}`,
  ];
  const summary = `release_proof_request:
  mode: ${mode}
  repo: ${repo}
  workflow: ${workflow}
  ref: ${ref}
  old_version: ${ctx.previousVersion}
  new_version: ${ctx.version}
  update_class: ${ctx.updateClass}
  appcast_url: ${ctx.publicAppcastUrl}
  channel: ${ctx.channel}
  channel_appcast: ${ctx.policy.appcast}
  channel_appcast_url: ${ctx.env.ONECONTEXT_SPARKLE_FEED_URL ?? ""}
  proof_reason: ${proofReason}
gh_command: ${quoteCommand(cmd)}`;
  fs.writeFileSync(transcript, summary, "utf8");
  process.stdout.write(summary);
  if (mode === "dry-run") {
    writeStageTiming(ctx, "prove", "dry-run", startedMs);
    return;
  }

  await timeReleaseStep(ctx, "prove", "gh_auth_status", () => runCommand("gh", ["auth", "status", "--hostname", "github.com"], { env: ctxEnv(ctx), stdout: "ignore" }));
  const runsBeforeJson = await runCapture("gh", ["run", "list", "--repo", repo, "--workflow", workflow, "--event", "workflow_dispatch", "--limit", "50", "--json", "databaseId"], { env: ctxEnv(ctx) });
  const dispatchStartedAt = new Date();
  const dispatchCommand = cmd[0] ?? fail("Proof dispatch command is empty.");
  await timeReleaseStep(ctx, "prove", "dispatch_workflow", () => runCommand(dispatchCommand, cmd.slice(1), { env: ctxEnv(ctx) }));
  await new Promise((resolve) => setTimeout(resolve, 8000));
  const runsJson = await runCapture("gh", ["run", "list", "--repo", repo, "--workflow", workflow, "--event", "workflow_dispatch", "--limit", "20", "--json", "databaseId,url,status,conclusion,createdAt,headBranch"], { env: ctxEnv(ctx) });
  const before = new Set((JSON.parse(runsBeforeJson) as Array<{ databaseId?: number }>).map((run) => String(run.databaseId ?? "")));
  const runs = JSON.parse(runsJson) as Array<{ databaseId?: number; createdAt: string; headBranch?: string }>;
  const candidates = runs
    .filter((run) => {
      const databaseId = String(run.databaseId ?? "");
      const created = new Date(run.createdAt);
      const headBranch = run.headBranch ?? "";
      return databaseId && !before.has(databaseId) && created >= dispatchStartedAt && (!headBranch || headBranch === ref);
    })
    .map((run) => String(run.databaseId));
  assertRelease(candidates.length === 1, candidates.length > 1 ? `Multiple new workflow_dispatch runs matched this request (${candidates.join(",")}); refusing to watch the wrong run.` : "Could not find a workflow_dispatch run to watch.");
  const runId = candidates[0] ?? fail("Could not find a workflow_dispatch run to watch.");
  fs.appendFileSync(transcript, `watching_run_id=${runId}\n`);
  await timeReleaseStep(ctx, "prove", "watch_workflow", () => runCommand("gh", ["run", "watch", runId, "--repo", repo, "--exit-status"], { env: ctxEnv(ctx) }));
  const artifactDir = path.join(ctx.evidenceDir, `self-hosted-run-${runId}`);
  fs.mkdirSync(artifactDir, { recursive: true });
  await timeReleaseStep(ctx, "prove", "download_proof_artifacts", () => runCommand("gh", ["run", "download", runId, "--repo", repo, "--dir", artifactDir], { env: ctxEnv(ctx) }));
  fs.appendFileSync(transcript, `artifact_dir=${artifactDir}\n`);
  await timeReleaseStep(ctx, "prove", "collect_proof_results", () => collectDownloadedProofResults(ctx, artifactDir));
  await writeSparkleFixtureProofResults(ctx);
  writeReleaseEvidence(ctx, "prove");
  await timeReleaseStep(ctx, "prove", "redact_evidence", () => runCommand(fromRoot("release", "tools", "redact-evidence.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  await timeReleaseStep(ctx, "prove", "audit_evidence_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  writeStageTiming(ctx, "prove", "passed", startedMs);
}

export async function phaseAudit(ctx: ReleaseContext): Promise<void> {
  const startedMs = Date.now();
  if (ctx.channel !== "official") {
    fail(`audit --channel ${ctx.channel} is not wired yet; only official public audit is active.`);
  }
  await timeReleaseStep(ctx, "audit", "validate_preflight", () => releaseValidate(ctx));
  await timeReleaseStep(ctx, "audit", "audit_public_release_assets", () => auditPublicReleaseAssets(ctx, ctx.tag));
  if (fs.existsSync(ctx.evidenceDir)) {
    await timeReleaseStep(ctx, "audit", "audit_evidence_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  }
  writeStageTiming(ctx, "audit", "passed", startedMs);
}

export async function phaseBless(ctx: ReleaseContext): Promise<void> {
  const startedMs = Date.now();
  if (ctx.channel !== "official") {
    fail(`bless --channel ${ctx.channel} is only valid for the official channel.`);
  }
  await timeReleaseStep(ctx, "bless", "validate_preflight", () => releaseValidate(ctx));
  for (const required of [ctx.releaseEvidence, ctx.assetManifest, ctx.runnerAttestation, path.join(ctx.evidenceDir, "redaction-report.json")]) {
    assertRelease(fs.existsSync(required), `Bless requires evidence file: ${required}`);
  }
  assertRelease(fs.existsSync(ctx.proofResultsDir) && fs.readdirSync(ctx.proofResultsDir).some((name) => name.endsWith(".json")), `Bless requires proof result JSON files in ${ctx.proofResultsDir}.`);
  for (const matrixCase of matrixCases(ctx.manifest)) {
    const required = path.join(ctx.proofResultsDir, `${matrixCase}.json`);
    assertRelease(fs.existsSync(required), `Bless requires updater matrix proof result: ${required}`);
  }
  const bad: string[] = [];
  for (const name of fs.readdirSync(ctx.proofResultsDir).filter((item) => item.endsWith(".json")).sort()) {
    const data = JSON.parse(fs.readFileSync(path.join(ctx.proofResultsDir, name), "utf8")) as Record<string, unknown>;
    const status = String(data.status ?? data.result ?? "").toLowerCase();
    if (!new Set(["passed", "pass", "ok"]).has(status)) {
      bad.push(`${name}: status=${status || "<missing>"}`);
    }
    if (data.expected_version && data.actual_version && data.expected_version !== data.actual_version) {
      bad.push(`${name}: actual_version=${data.actual_version} expected=${data.expected_version}`);
    }
  }
  assertRelease(bad.length === 0, `proof result failures: ${bad.join("; ")}`);
  await timeReleaseStep(ctx, "bless", "audit_evidence_redaction", () => runCommand(fromRoot("release", "tools", "audit-evidence-redaction.sh"), [ctx.evidenceDir], { env: ctxEnv(ctx) }));
  writeReleaseEvidence(ctx, "bless");
  writeStageTiming(ctx, "bless", "passed", startedMs);
  console.log(`release blessed: ${ctx.tag}`);
}

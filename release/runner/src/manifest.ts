import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { XMLParser } from "fast-xml-parser";
import { execa } from "execa";
import { parse as parseToml } from "smol-toml";
import { z } from "zod";
import { ReleaseError, assertRelease } from "./errors.js";
import { fromRoot, repoRoot } from "./paths.js";

export const SCHEMA_VERSION = "1context.release.v1";
export const STAGE_TIMING_SCHEMA = "1context.release-stage-timing.v1";
export const SPARKLE_NS = "http://www.andymatuschak.org/xml-namespaces/sparkle";

export const REQUIRED_PROOFS = new Set([
  "clean_tree",
  "release_manifest",
  "version_consistency",
  "release_policy",
  "full_tests",
  "package_smoke",
  "asset_audit",
  "self_hosted_gui_update",
  "real_uninstall_reinstall",
  "evidence_redaction",
  "runner_attestation",
]);

export const REQUIRED_RUNNER_LABELS = new Set([
  "self-hosted",
  "macOS",
  "ARM64",
  "onecontext-update-runner",
]);

export const REQUIRED_REDACTION_PATTERNS = new Set([
  "/Users/",
  "paulhan",
  ".codex",
  "Library/Application Support/1Context",
  "Library/Logs/1Context",
  "Library/Caches/1Context",
  "1context.sock",
  "SPARKLE_PRIVATE_ED_KEY",
  "GITHUB_TOKEN",
  "BEGIN PRIVATE KEY",
]);

export const REQUIRED_MATRIX_CASES = new Set([
  "already_current_manual_check",
  "mandatory_automatic_success",
  "stale_sparkle_defaults",
  "old_app_with_new_appcast",
  "app_relaunch_recovery",
  "login_restart_recovery",
  "real_uninstall_reinstall",
]);

const REQUIRED_RELEASE_CHANNELS = new Set(["dev", "prototype", "private", "official"]);
const CHANNEL_BUDGET_KEYS = [
  "budget_validate_seconds",
  "budget_build_seconds",
  "budget_publish_seconds",
  "budget_prove_seconds",
  "budget_audit_seconds",
  "budget_bless_seconds",
] as const;

const semverPattern = /^\d+\.\d+\.\d+$/;
const coreFallbackPattern = /fallback\s*=\s*"([^"]+)"/;

const ChannelSchema = z.object({
  description: z.string().min(1),
  requires_clean_tree: z.boolean(),
  requires_tag: z.boolean(),
  signing_mode: z.enum(["adhoc", "apple-development", "developer-id"]),
  notarize: z.boolean(),
  appcast: z.enum(["none", "private", "public"]),
  public_asset_mutation: z.boolean(),
  proof: z.string().min(1),
  managed_postgres: z.enum(["off", "auto", "required"]).default("required"),
  budget_is_advisory: z.boolean(),
  budget_validate_seconds: z.number().int().nonnegative(),
  budget_build_seconds: z.number().int().nonnegative(),
  budget_publish_seconds: z.number().int().nonnegative(),
  budget_prove_seconds: z.number().int().nonnegative(),
  budget_audit_seconds: z.number().int().nonnegative(),
  budget_bless_seconds: z.number().int().nonnegative(),
  artifact_repo: z.string().optional(),
  artifact_repo_visibility: z.string().optional(),
  appcast_authentication: z.string().optional(),
  private_appcast_url: z.string().optional(),
  private_download_url_prefix: z.string().optional(),
  private_link_url: z.string().optional(),
});

const ManifestSchema = z.object({
  schema_version: z.literal(SCHEMA_VERSION),
  version: z.string(),
  previous_version: z.string(),
  tag: z.string(),
  update_class: z.enum(["mandatory", "optional"]),
  approved_by: z.string().min(1),
  reason: z.string().min(1),
  reason_detail: z.string().min(1),
  minimum_autoupdate_version: z.string().optional().default(""),
  minimum_update_version: z.string().optional().default(""),
  critical_update_version: z.string().optional().default(""),
  public_appcast_url: z.string(),
  stable_dmg_name: z.string(),
  required_proofs: z.array(z.string()),
  required_runner_labels: z.array(z.string()),
  release_factory: z.object({
    default_channel: z.string(),
    forbid_backwards_compatibility_shims: z.boolean(),
    stage_timing_schema: z.string(),
    channels: z.record(z.string(), ChannelSchema),
  }),
  release_notes_policy: z.object({
    show_in_update_window: z.boolean(),
    public_notes_file: z.string().min(1),
  }),
  update_ui: z.object({
    optional_prompt: z.object({
      title: z.string().min(1),
      body: z.string().min(1),
    }),
    failure_message: z.object({
      title: z.string(),
      body: z.string(),
    }),
    post_install_message: z.object({
      enabled: z.boolean(),
      title: z.string(),
      body: z.string().optional().default(""),
    }),
  }),
  evidence_redaction_policy: z.object({
    require_redaction: z.boolean(),
    forbidden_patterns: z.array(z.string()),
  }),
  updater_matrix: z.array(z.object({
    case: z.string().min(1),
    expected_version: z.string(),
    proof: z.string().min(1),
    description: z.string().min(1),
  })),
});

export type ChannelPolicy = z.infer<typeof ChannelSchema>;
export type ReleaseManifest = z.infer<typeof ManifestSchema>;

export interface ManifestPaths {
  root: string;
  manifest: string;
  versionFile: string;
  coreFile: string;
  releaseNotes: string;
  releaseWorkflow: string;
  proofWorkflow: string;
  privateProofWorkflow: string;
}

export interface ManifestOptions {
  root?: string;
  manifest?: string;
  versionFile?: string;
  coreFile?: string;
  releaseNotes?: string;
  releaseWorkflow?: string;
  proofWorkflow?: string;
  privateProofWorkflow?: string;
  appcast?: string;
  channel?: string;
  requireClean?: boolean;
}

export function defaultManifestPaths(options: ManifestOptions = {}): ManifestPaths {
  const root = path.resolve(options.root ?? repoRoot);
  return {
    root,
    manifest: path.resolve(options.manifest ?? path.join(root, "release", "release.toml")),
    versionFile: path.resolve(options.versionFile ?? path.join(root, "VERSION")),
    coreFile: path.resolve(options.coreFile ?? path.join(root, "macos", "Sources", "OneContextCore", "Core.swift")),
    releaseNotes: path.resolve(options.releaseNotes ?? path.join(root, "RELEASE_NOTES.md")),
    releaseWorkflow: path.resolve(options.releaseWorkflow ?? path.join(root, ".github", "workflows", "release.yml")),
    proofWorkflow: path.resolve(options.proofWorkflow ?? path.join(root, ".github", "workflows", "self-hosted-mac-update-proof.yml")),
    privateProofWorkflow: path.resolve(options.privateProofWorkflow ?? path.join(root, ".github", "workflows", "self-hosted-mac-private-update-proof.yml")),
  };
}

export function loadManifestFile(manifestPath = fromRoot("release", "release.toml")): ReleaseManifest {
  let parsed: unknown;
  try {
    parsed = parseToml(fs.readFileSync(manifestPath, "utf8"));
  } catch (error) {
    throw new ReleaseError(`release manifest is not valid TOML: ${error instanceof Error ? error.message : String(error)}`);
  }
  const result = ManifestSchema.safeParse(parsed);
  if (!result.success) {
    throw new ReleaseError(`release manifest schema error: ${z.prettifyError(result.error)}`);
  }
  return result.data;
}

function readText(filePath: string, label: string): string {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch (error) {
    throw new ReleaseError(`${label} not found: ${filePath}`);
  }
}

function semverTuple(version: string): [number, number, number] {
  assertRelease(semverPattern.test(version), `Version must look like 0.1.64, got ${JSON.stringify(version)}.`);
  return version.split(".").map((part) => Number.parseInt(part, 10)) as [number, number, number];
}

function compareSemver(left: string, right: string): number {
  const a = semverTuple(left);
  const b = semverTuple(right);
  for (let index = 0; index < 3; index += 1) {
    const diff = (a[index] ?? 0) - (b[index] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

function missingFrom(required: Set<string>, actual: Iterable<string>): string[] {
  const actualSet = new Set(actual);
  return [...required].filter((item) => !actualSet.has(item)).sort();
}

export function channelPolicy(manifest: ReleaseManifest, channelName = ""): [string, ChannelPolicy] {
  const name = (channelName || manifest.release_factory.default_channel).trim();
  assertRelease(name.length > 0, "Release channel must not be empty.");
  const policy = manifest.release_factory.channels[name];
  assertRelease(policy, `Unknown release channel ${JSON.stringify(name)}.`);
  return [name, policy];
}

export function validateManifestShape(manifest: ReleaseManifest): void {
  const version = manifest.version;
  const previousVersion = manifest.previous_version;
  semverTuple(version);
  assertRelease(compareSemver(previousVersion, version) < 0, `previous_version ${previousVersion} must be older than version ${version}.`);
  assertRelease(manifest.tag === `v${version}`, `Manifest tag ${JSON.stringify(manifest.tag)} must be v${version}.`);
  assertRelease(
    manifest.public_appcast_url === "https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml",
    "public_appcast_url must be the public latest/download appcast URL.",
  );
  assertRelease(manifest.stable_dmg_name === "1Context.dmg", 'stable_dmg_name must be "1Context.dmg".');

  if (manifest.update_class === "mandatory") {
    assertRelease(manifest.minimum_autoupdate_version === previousVersion, "Mandatory releases must minimum-autoupdate from previous_version.");
    assertRelease(manifest.critical_update_version === version, "Mandatory releases must set critical_update_version to version.");
  } else {
    assertRelease(!manifest.minimum_autoupdate_version, "Optional releases must not set minimum_autoupdate_version.");
    assertRelease(!manifest.critical_update_version, "Optional releases must not set critical_update_version.");
  }
  if (manifest.minimum_update_version) {
    semverTuple(manifest.minimum_update_version);
  }

  const missingProofs = missingFrom(REQUIRED_PROOFS, manifest.required_proofs);
  assertRelease(missingProofs.length === 0, `Manifest is missing required proofs: ${missingProofs.join(", ")}`);
  const missingLabels = missingFrom(REQUIRED_RUNNER_LABELS, manifest.required_runner_labels);
  assertRelease(missingLabels.length === 0, `Manifest is missing runner labels: ${missingLabels.join(", ")}`);

  validateReleaseFactory(manifest);

  const failure = manifest.update_ui.failure_message;
  assertRelease(failure.title === "Update failed.", 'update_ui.failure_message.title must be "Update failed."');
  assertRelease(failure.body === "Please contact support at paul@haptica.ai.", "update_ui.failure_message.body must direct users to paul@haptica.ai.");
  assertRelease(manifest.update_ui.post_install_message.title === "1Context Improved!", 'update_ui.post_install_message.title must default to "1Context Improved!"');

  assertRelease(manifest.evidence_redaction_policy.require_redaction, "evidence_redaction_policy.require_redaction must be true.");
  const missingPatterns = missingFrom(REQUIRED_REDACTION_PATTERNS, manifest.evidence_redaction_policy.forbidden_patterns);
  assertRelease(missingPatterns.length === 0, `Evidence redaction policy is missing patterns: ${missingPatterns.join(", ")}`);

  const cases = manifest.updater_matrix.map((item) => item.case);
  const missingCases = missingFrom(REQUIRED_MATRIX_CASES, cases);
  assertRelease(missingCases.length === 0, `Updater matrix is missing cases: ${missingCases.join(", ")}`);
  for (const item of manifest.updater_matrix) {
    semverTuple(item.expected_version);
  }
}

function validateReleaseFactory(manifest: ReleaseManifest): void {
  const factory = manifest.release_factory;
  assertRelease(factory.default_channel === "official", 'release_factory.default_channel must be "official".');
  assertRelease(factory.forbid_backwards_compatibility_shims, "release_factory.forbid_backwards_compatibility_shims must be true.");
  assertRelease(factory.stage_timing_schema === STAGE_TIMING_SCHEMA, `release_factory.stage_timing_schema must be ${JSON.stringify(STAGE_TIMING_SCHEMA)}.`);

  const missingChannels = missingFrom(REQUIRED_RELEASE_CHANNELS, Object.keys(factory.channels));
  assertRelease(missingChannels.length === 0, `release_factory.channels is missing: ${missingChannels.join(", ")}`);

  for (const name of [...REQUIRED_RELEASE_CHANNELS].sort()) {
    const policy = factory.channels[name];
    assertRelease(policy, `release_factory.channels.${name} must be a table.`);
    for (const key of CHANNEL_BUDGET_KEYS) {
      assertRelease(Number.isInteger(policy[key]) && policy[key] >= 0, `release_factory.channels.${name}.${key} must be non-negative.`);
    }
    if (name === "dev") {
      assertRelease(
        !policy.requires_clean_tree && !policy.requires_tag && policy.signing_mode === "apple-development" && !policy.notarize &&
          policy.appcast === "none" && !policy.public_asset_mutation,
        "dev channel must stay local, Apple Development signed, unnotarized, appcast-free, and non-mutating.",
      );
    }
    assertRelease(policy.managed_postgres === "required", `release_factory.channels.${name}.managed_postgres must be "required" for release-factory builds.`);
    if (name === "prototype") {
      assertRelease(!policy.requires_tag && policy.appcast === "none" && !policy.public_asset_mutation, "prototype channel must not require a tag, appcast, or public asset mutation.");
      assertRelease(policy.signing_mode === "developer-id" && policy.notarize, "prototype channel must produce a Developer ID notarized DMG.");
    }
    if (name === "private") {
      const repo = policy.artifact_repo ?? "";
      assertRelease(/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repo), "private channel artifact_repo must be a GitHub owner/repo name.");
      assertRelease(policy.artifact_repo_visibility === "public", "private channel artifact_repo_visibility must be public so Sparkle can fetch the appcast without auth.");
      assertRelease(policy.appcast_authentication === "none", "private channel appcast_authentication must be none; shipped Sparkle feeds must be ordinary HTTPS.");
      assertRelease(policy.appcast === "private" && Boolean(policy.private_appcast_url), "private channel must define a private appcast URL.");
      assertRelease(policy.private_appcast_url === `https://github.com/${repo}/releases/latest/download/appcast.xml`, "private channel appcast URL must be the artifact repo latest appcast URL.");
      assertRelease(policy.private_download_url_prefix === `https://github.com/${repo}/releases/download/${manifest.tag}/`, "private channel download prefix must be the artifact repo release tag URL.");
      assertRelease(policy.private_link_url === `https://github.com/${repo}/releases/tag/${manifest.tag}`, "private channel link URL must be the artifact repo release tag URL.");
      assertRelease(!policy.public_asset_mutation, "private channel must not mutate public assets.");
      assertRelease(policy.signing_mode === "developer-id" && policy.notarize, "private channel must produce Developer ID notarized assets.");
    }
    if (name === "official") {
      assertRelease(policy.requires_clean_tree && policy.requires_tag, "official channel must require a clean tagged tree.");
      assertRelease(policy.signing_mode === "developer-id" && policy.notarize && policy.appcast === "public" && policy.public_asset_mutation, "official channel must be Developer ID notarized, public-appcast, and public-mutating.");
    }
  }
}

export function validateVersionFiles(manifest: ReleaseManifest, paths: ManifestPaths): void {
  const versionText = readText(paths.versionFile, "VERSION file").trim();
  assertRelease(versionText === manifest.version, `VERSION ${JSON.stringify(versionText)} does not match release manifest ${JSON.stringify(manifest.version)}.`);

  const coreText = readText(paths.coreFile, "Core.swift");
  const match = coreFallbackPattern.exec(coreText);
  assertRelease(match, "Core.swift does not expose a fallback version.");
  assertRelease(match[1] === manifest.version, `Core.swift fallback ${JSON.stringify(match[1])} does not match release manifest ${JSON.stringify(manifest.version)}.`);

  const releaseNotes = readText(paths.releaseNotes, "release notes");
  const firstLine = releaseNotes.split(/\r?\n/)[0] ?? "";
  assertRelease(firstLine.includes(manifest.version) || firstLine.includes(`v${manifest.version}`), `Release notes heading must mention ${manifest.version}.`);
}

export function validateWorkflows(manifest: ReleaseManifest, paths: ManifestPaths): void {
  const releaseText = readText(paths.releaseWorkflow, "release workflow");
  for (const fragment of [
    "./scripts/release-train.sh validate",
    "./scripts/release-train.sh build --channel official",
    "./scripts/release-train.sh publish",
  ]) {
    assertRelease(releaseText.includes(fragment), `Release workflow must invoke ${fragment}.`);
  }
  for (const label of REQUIRED_RUNNER_LABELS) {
    assertRelease(releaseText.includes(label), `Release workflow is missing runner label ${label}.`);
  }
  const proofText = readText(paths.proofWorkflow, "self-hosted proof workflow");
  validateProofWorkflowText(proofText, paths.proofWorkflow, "./scripts/release-train.sh prove --runner-execute");
  const privateProofText = readText(paths.privateProofWorkflow, "self-hosted private proof workflow");
  validateProofWorkflowText(privateProofText, paths.privateProofWorkflow, "./scripts/release-train.sh prove --channel private --runner-execute");
  void manifest;
}

function validateProofWorkflowText(text: string, workflowPath: string, runnerCommand: string): void {
  assertRelease(text.includes(runnerCommand), `${workflowPath} must execute through ${runnerCommand}.`);
  assertRelease(text.includes("proof_reason:"), `${workflowPath} must require a proof_reason input.`);
  const forbiddenInputs = [
    "old_version:",
    "new_version:",
    "staging_appcast_url:",
    "update_class:",
    "old_tag:",
    "old_dmg_url:",
    "update_timeout_seconds:",
    "steady_state_seconds:",
    "artifact_retention_days:",
  ];
  for (const fragment of forbiddenInputs) {
    assertRelease(!text.includes(fragment), `${workflowPath} must not expose release fact input ${fragment}`);
  }
  const forbiddenEnvs = [
    "ONECONTEXT_OLD_VERSION:",
    "ONECONTEXT_NEW_VERSION:",
    "ONECONTEXT_OLD_TAG:",
    "ONECONTEXT_OLD_DMG_URL:",
    "ONECONTEXT_STAGING_APPCAST_URL:",
    "ONECONTEXT_EXPECTED_UPDATE_CLASS:",
    "ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS:",
    "ONECONTEXT_STEADY_STATE_SECONDS:",
  ];
  for (const fragment of forbiddenEnvs) {
    assertRelease(!text.includes(fragment), `${workflowPath} must not pass manual release env ${fragment}`);
  }
  for (const label of REQUIRED_RUNNER_LABELS) {
    assertRelease(text.includes(label), `${workflowPath} is missing runner label ${label}.`);
  }
}

function asRecord(value: unknown, label: string): Record<string, unknown> {
  assertRelease(value && typeof value === "object" && !Array.isArray(value), `${label} is missing or malformed.`);
  return value as Record<string, unknown>;
}

function firstRecord(value: unknown, label: string): Record<string, unknown> {
  if (Array.isArray(value)) {
    assertRelease(value.length > 0, `${label} is empty.`);
    return asRecord(value[0], label);
  }
  return asRecord(value, label);
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

export function validateAppcast(manifest: ReleaseManifest, appcastPath: string, channelName = ""): void {
  assertRelease(fs.existsSync(appcastPath), `Appcast not found: ${appcastPath}`);
  let root: Record<string, unknown>;
  try {
    root = asRecord(new XMLParser({
      ignoreAttributes: false,
      attributeNamePrefix: "@",
      parseTagValue: false,
      trimValues: true,
    }).parse(fs.readFileSync(appcastPath, "utf8")), "appcast");
  } catch (error) {
    throw new ReleaseError(`Appcast is not valid XML: ${error instanceof Error ? error.message : String(error)}`);
  }
  const rss = asRecord(root.rss, "Appcast rss");
  const xmlChannel = firstRecord(asRecord(rss.channel, "Appcast channel"), "Appcast channel");
  const item = firstRecord(xmlChannel.item, "Appcast channel/item");
  const version = stringValue(item["sparkle:version"]);
  assertRelease(version === manifest.version, `Appcast version ${JSON.stringify(version)} does not match manifest ${JSON.stringify(manifest.version)}.`);

  const [releaseChannel, channelData] = channelPolicy(manifest, channelName);
  assertRelease(channelData.appcast !== "none", `Channel ${releaseChannel} must not produce an appcast.`);

  const critical = item["sparkle:criticalUpdate"];
  if (manifest.update_class === "mandatory") {
    const criticalRecord = firstRecord(critical, "Appcast criticalUpdate");
    assertRelease(stringValue(criticalRecord["@sparkle:version"]) === manifest.critical_update_version, "Appcast criticalUpdate version does not match manifest.");
  } else {
    assertRelease(critical === undefined, "Optional manifest must not produce sparkle:criticalUpdate.");
  }

  const minimumAutoupdate = stringValue(item["sparkle:minimumAutoupdateVersion"]);
  assertRelease(minimumAutoupdate === manifest.minimum_autoupdate_version, "Appcast minimumAutoupdateVersion does not match manifest.");

  const enclosure = firstRecord(item.enclosure, "Appcast enclosure");
  const enclosureUrl = stringValue(enclosure["@url"]);
  const enclosureLength = stringValue(enclosure["@length"]);
  assertRelease(/^\d+$/.test(enclosureLength) && Number(enclosureLength) > 0, "Appcast enclosure must include a positive length.");
  assertRelease(stringValue(enclosure["@sparkle:edSignature"]).trim().length > 0, "Appcast enclosure is missing sparkle:edSignature.");

  const expectedAsset = `1Context-${manifest.version}-macos-arm64.dmg`;
  const enclosureAsset = path.basename(new URL(enclosureUrl).pathname);
  assertRelease(enclosureAsset === expectedAsset, `Appcast enclosure asset ${JSON.stringify(enclosureAsset)} does not match ${JSON.stringify(expectedAsset)}.`);
  const expectedUrl = channelData.appcast === "private"
    ? `${channelData.private_download_url_prefix ?? ""}${expectedAsset}`
    : `https://github.com/hapticasensorics/1context/releases/download/v${manifest.version}/${expectedAsset}`;
  assertRelease(enclosureUrl === expectedUrl, `Appcast enclosure url ${JSON.stringify(enclosureUrl)} does not match ${JSON.stringify(expectedUrl)}.`);

  const description = item.description;
  if (!manifest.release_notes_policy.show_in_update_window && description !== undefined) {
    assertRelease(stringValue(description).trim().length === 0, "Manifest hides updater release notes, but appcast contains a description.");
  }
}

export async function checkCleanTree(root = repoRoot): Promise<void> {
  const result = await execa("git", ["-C", root, "status", "--porcelain=v1", "--untracked-files=all"]);
  const dirty = result.stdout.split(/\r?\n/).filter((line) => line.trim().length > 0);
  if (dirty.length > 0) {
    throw new ReleaseError(`Release tree is dirty; commit or remove changes before release:\n${dirty.slice(0, 20).join("\n")}`);
  }
}

export async function checkSourcedHelpers(root = repoRoot): Promise<void> {
  const listed = await execa("git", ["-C", root, "ls-files", "*.sh"]);
  const files = listed.stdout.split(/\r?\n/).filter(Boolean);
  let testText = "";
  for (const testPath of fs.readdirSync(path.join(root, "scripts")).filter((name) => /^test.*\.sh$/.test(name))) {
    testText += fs.readFileSync(path.join(root, "scripts", testPath), "utf8") + "\n";
  }
  const references = new Set<string>();
  const pattern = /(?:^|[;&|\s])(?:source|\.)\s+["']?(?:\$ROOT\/)?([^"'\s]+\.sh)/g;
  for (const relative of files) {
    const filePath = path.join(root, relative);
    if (!fs.existsSync(filePath)) continue;
    const text = fs.readFileSync(filePath, "utf8");
    for (const match of text.matchAll(pattern)) {
      let helper = match[1] ?? "";
      if (helper.startsWith("./")) helper = helper.slice(2);
      if (helper.startsWith("scripts/")) references.add(helper);
    }
  }
  for (const helper of [...references].sort()) {
    const tracked = await execa("git", ["-C", root, "ls-files", "--error-unmatch", helper], { reject: false });
    assertRelease(tracked.exitCode === 0, `Sourced shell helper is not tracked by Git: ${helper}`);
    assertRelease(testText.includes(helper) || testText.includes(path.basename(helper)), `Sourced shell helper is not referenced by a shell test: ${helper}`);
  }
}

export async function validateManifest(options: ManifestOptions = {}): Promise<ReleaseManifest> {
  const paths = defaultManifestPaths(options);
  const manifest = loadManifestFile(paths.manifest);
  validateManifestShape(manifest);
  validateVersionFiles(manifest, paths);
  validateWorkflows(manifest, paths);
  await checkSourcedHelpers(paths.root);
  if (options.appcast) {
    validateAppcast(manifest, path.resolve(options.appcast), options.channel ?? "");
  }
  if (options.requireClean) {
    await checkCleanTree(paths.root);
  }
  return manifest;
}

export function envForManifest(manifest: ReleaseManifest, manifestPath: string, channelName = ""): Record<string, string> {
  const [channel, channelData] = channelPolicy(manifest, channelName);
  const channelAppcastMode = channelData.appcast;
  let channelAppcastUrl = "";
  let downloadUrlPrefix = "";
  let releaseNotesUrlPrefix = "";
  let linkUrl = "";
  if (channelAppcastMode === "public") {
    channelAppcastUrl = manifest.public_appcast_url;
    downloadUrlPrefix = `https://github.com/hapticasensorics/1context/releases/download/${manifest.tag}/`;
    releaseNotesUrlPrefix = downloadUrlPrefix;
    linkUrl = `https://github.com/hapticasensorics/1context/releases/tag/${manifest.tag}`;
  } else if (channelAppcastMode === "private") {
    channelAppcastUrl = channelData.private_appcast_url ?? "";
    downloadUrlPrefix = channelData.private_download_url_prefix ?? "";
    releaseNotesUrlPrefix = downloadUrlPrefix;
    linkUrl = channelData.private_link_url ?? "";
  }

  const budgetEnv = Object.fromEntries(CHANNEL_BUDGET_KEYS.map((key) => [
    `ONECONTEXT_RELEASE_${key.replace(/^budget_/, "").toUpperCase()}`,
    String(channelData[key]),
  ]));

  return {
    ONECONTEXT_RELEASE_MANIFEST: manifestPath,
    ONECONTEXT_RELEASE_VERSION: manifest.version,
    ONECONTEXT_RELEASE_PREVIOUS_VERSION: manifest.previous_version,
    ONECONTEXT_RELEASE_TAG: manifest.tag,
    ONECONTEXT_RELEASE_UPDATE_CLASS: manifest.update_class,
    ONECONTEXT_RELEASE_PUBLIC_APPCAST_URL: manifest.public_appcast_url,
    ONECONTEXT_RELEASE_STABLE_DMG_NAME: manifest.stable_dmg_name,
    ONECONTEXT_RELEASE_DEFAULT_CHANNEL: manifest.release_factory.default_channel,
    ONECONTEXT_RELEASE_CHANNEL: channel,
    ONECONTEXT_APP_IDENTITY: channel === "dev" ? "dev" : "official",
    ONECONTEXT_APP_BUNDLE_NAME: channel === "dev" ? "1Context Dev" : "1Context",
    ONECONTEXT_APP_DISPLAY_NAME: channel === "dev" ? "1Context Dev" : "1Context",
    ONECONTEXT_BUNDLE_IDENTIFIER: channel === "dev" ? "com.haptica.1context.dev" : "com.haptica.1context",
    ONECONTEXT_LOCAL_WEB_PROXY_LABEL: channel === "dev"
      ? "com.haptica.1context.dev.local-web-proxy"
      : "com.haptica.1context.local-web-proxy",
    ONECONTEXT_DMG_VOLUME_NAME: channel === "dev" ? "1Context Dev" : "1Context",
    ONECONTEXT_EXPECTED_APP_BASENAME: channel === "dev" ? "1Context Dev.app" : "1Context.app",
    ONECONTEXT_INCLUDE_MANAGED_POSTGRES: channelData.managed_postgres === "off"
      ? "false"
      : channelData.managed_postgres === "auto"
        ? "auto"
        : "true",
    ONECONTEXT_RELEASE_CHANNEL_REQUIRES_CLEAN_TREE: channelData.requires_clean_tree ? "1" : "0",
    ONECONTEXT_RELEASE_CHANNEL_REQUIRES_TAG: channelData.requires_tag ? "1" : "0",
    ONECONTEXT_RELEASE_CHANNEL_SIGNING_MODE: channelData.signing_mode,
    ONECONTEXT_RELEASE_CHANNEL_NOTARIZE: channelData.notarize ? "1" : "0",
    ONECONTEXT_RELEASE_CHANNEL_APPCAST: channelAppcastMode,
    ONECONTEXT_RELEASE_CHANNEL_APPCAST_URL: channelAppcastUrl,
    ONECONTEXT_RELEASE_CHANNEL_PUBLIC_ASSET_MUTATION: channelData.public_asset_mutation ? "1" : "0",
    ONECONTEXT_RELEASE_CHANNEL_PROOF: channelData.proof,
    ONECONTEXT_RELEASE_CHANNEL_ARTIFACT_REPO: channelData.artifact_repo ?? "",
    ONECONTEXT_RELEASE_BUDGET_ADVISORY: channelData.budget_is_advisory ? "1" : "0",
    ONECONTEXT_RELEASE_STAGE_TIMING_SCHEMA: manifest.release_factory.stage_timing_schema,
    ONECONTEXT_SPARKLE_FEED_URL: channelAppcastUrl,
    ONECONTEXT_SPARKLE_MANDATORY: manifest.update_class === "mandatory" ? "1" : "0",
    ONECONTEXT_SPARKLE_MANDATORY_FROM_VERSION: manifest.critical_update_version,
    ONECONTEXT_SPARKLE_MINIMUM_AUTOUPDATE_VERSION: manifest.minimum_autoupdate_version,
    ONECONTEXT_SPARKLE_MINIMUM_UPDATE_VERSION: manifest.minimum_update_version,
    ONECONTEXT_SPARKLE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW: manifest.release_notes_policy.show_in_update_window ? "1" : "0",
    ONECONTEXT_UPDATE_OPTIONAL_PROMPT_TITLE: manifest.update_ui.optional_prompt.title,
    ONECONTEXT_UPDATE_OPTIONAL_PROMPT_BODY: manifest.update_ui.optional_prompt.body,
    ONECONTEXT_UPDATE_FAILURE_TITLE: manifest.update_ui.failure_message.title,
    ONECONTEXT_UPDATE_FAILURE_BODY: manifest.update_ui.failure_message.body,
    ONECONTEXT_UPDATE_POST_INSTALL_MESSAGE_ENABLED: manifest.update_ui.post_install_message.enabled ? "1" : "0",
    ONECONTEXT_UPDATE_POST_INSTALL_TITLE: manifest.update_ui.post_install_message.title,
    ONECONTEXT_UPDATE_POST_INSTALL_BODY: manifest.update_ui.post_install_message.body,
    SPARKLE_DOWNLOAD_URL_PREFIX: downloadUrlPrefix,
    SPARKLE_RELEASE_NOTES_URL_PREFIX: releaseNotesUrlPrefix,
    SPARKLE_LINK_URL: linkUrl,
    ...budgetEnv,
  };
}

export function shellExport(values: Record<string, string>): string {
  return Object.entries(values).map(([key, value]) => `export ${key}=${shellQuote(value)}`).join("\n") + "\n";
}

export function shellQuote(value: string): string {
  if (/^[A-Za-z0-9_./:@%+=,-]*$/.test(value)) return value || "''";
  return `'${value.replaceAll("'", "'\"'\"'")}'`;
}

export function forbiddenPatterns(manifest: ReleaseManifest): string[] {
  validateManifestShape(manifest);
  return manifest.evidence_redaction_policy.forbidden_patterns;
}

export function matrixCases(manifest: ReleaseManifest): string[] {
  validateManifestShape(manifest);
  return manifest.updater_matrix.map((item) => item.case);
}

export async function writeAssetManifest(manifest: ReleaseManifest, distDir: string, output: string): Promise<void> {
  const names = [
    `1Context-${manifest.version}-macos-arm64.dmg`,
    `1Context-${manifest.version}-macos-arm64.dmg.sha256`,
    manifest.stable_dmg_name,
    `${manifest.stable_dmg_name}.sha256`,
    "appcast.xml",
  ];
  const assets = [];
  const missing = [];
  for (const name of names) {
    const assetPath = path.join(distDir, name);
    if (!fs.existsSync(assetPath)) {
      missing.push(name);
      continue;
    }
    const buffer = fs.readFileSync(assetPath);
    assets.push({
      name,
      path: `dist/${name}`,
      size: buffer.byteLength,
      sha256: crypto.createHash("sha256").update(buffer).digest("hex"),
    });
  }
  assertRelease(missing.length === 0, `Missing release assets: ${missing.join(", ")}`);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, JSON.stringify({
    schema_version: "1context.asset-manifest.v1",
    version: manifest.version,
    tag: manifest.tag,
    generated_at: new Date().toISOString(),
    assets,
  }, null, 2) + "\n");
}

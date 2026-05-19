import path from "node:path";
import { channelPolicy, defaultManifestPaths, envForManifest, loadManifestFile, type ChannelPolicy, type ReleaseManifest } from "./manifest.js";
import { fromRoot, repoRoot } from "./paths.js";

export interface ReleaseContextOptions {
  channel?: string;
  evidenceDir?: string;
  arch?: string;
}

export interface ReleaseContext {
  root: string;
  manifestPath: string;
  manifest: ReleaseManifest;
  channel: string;
  policy: ChannelPolicy;
  version: string;
  previousVersion: string;
  tag: string;
  updateClass: "mandatory" | "optional";
  publicAppcastUrl: string;
  stableDmgName: string;
  evidenceDir: string;
  proofResultsDir: string;
  assetManifest: string;
  runnerAttestation: string;
  releaseEvidence: string;
  arch: string;
  dmg: string;
  env: Record<string, string>;
}

export function createReleaseContext(options: ReleaseContextOptions = {}): ReleaseContext {
  const root = repoRoot;
  const manifestPath = defaultManifestPaths({ root }).manifest;
  const manifest = loadManifestFile(manifestPath);
  const [channel, policy] = channelPolicy(manifest, options.channel ?? "");
  const version = manifest.version;
  const evidenceDir = path.resolve(options.evidenceDir ?? process.env.ONECONTEXT_RELEASE_EVIDENCE_DIR ?? fromRoot("dist", "release-evidence", version));
  const arch = options.arch ?? process.env.ONECONTEXT_ARCH ?? "arm64";
  const env = {
    ...envForManifest(manifest, manifestPath, channel),
    ONECONTEXT_RELEASE_EVIDENCE_DIR: evidenceDir,
    ONECONTEXT_ARCH: arch,
  };
  return {
    root,
    manifestPath,
    manifest,
    channel,
    policy,
    version,
    previousVersion: manifest.previous_version,
    tag: manifest.tag,
    updateClass: manifest.update_class,
    publicAppcastUrl: manifest.public_appcast_url,
    stableDmgName: manifest.stable_dmg_name,
    evidenceDir,
    proofResultsDir: path.join(evidenceDir, "proof-results"),
    assetManifest: path.join(evidenceDir, "asset-manifest.json"),
    runnerAttestation: path.join(evidenceDir, "runner-attestation.json"),
    releaseEvidence: path.join(evidenceDir, "release-evidence.json"),
    arch,
    dmg: fromRoot("dist", `1Context-${version}-macos-${arch}.dmg`),
    env,
  };
}

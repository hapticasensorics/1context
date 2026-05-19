#!/usr/bin/env node
import { Command } from "commander";
import path from "node:path";
import { createReleaseContext } from "./context.js";
import { ReleaseError } from "./errors.js";
import {
  checkCleanTree,
  checkSourcedHelpers,
  defaultManifestPaths,
  envForManifest,
  forbiddenPatterns,
  loadManifestFile,
  type ManifestOptions,
  matrixCases,
  shellExport,
  validateManifest,
  writeAssetManifest,
  writeFixtureProofResults,
} from "./manifest.js";
import { phaseAudit, phaseBless, phaseBuild, phaseProve, phasePublish, phaseValidate } from "./phases.js";

type ChannelOptions = { channel?: string };

function addChannelOption<T extends Command>(command: T): T {
  return command.option("--channel <name>", "release channel");
}

function commandChannel(command: Command, options: ChannelOptions): string {
  return options.channel ?? (command.parent?.opts<{ channel?: string }>().channel ?? argvOption("channel") ?? "");
}

function contextFor(command: Command, options: ChannelOptions) {
  return createReleaseContext({ channel: commandChannel(command, options) });
}

function addManifestCommonOptions<T extends Command>(command: T): T {
  return addChannelOption(command)
    .option("--root <path>", "repo root")
    .option("--manifest <path>", "release manifest path")
    .option("--version-file <path>", "VERSION file path")
    .option("--core-file <path>", "Core.swift file path")
    .option("--release-notes <path>", "release notes path")
    .option("--release-workflow <path>", "release workflow path")
    .option("--proof-workflow <path>", "public proof workflow path")
    .option("--private-proof-workflow <path>", "private proof workflow path")
    .option("--appcast <path>", "appcast path")
    .option("--require-clean", "require a clean git tree");
}

function manifestOptions(options: Record<string, unknown>): ManifestOptions {
  const out: ManifestOptions = {};
  for (const [source, target] of [
    ["root", "root"],
    ["manifest", "manifest"],
    ["versionFile", "versionFile"],
    ["coreFile", "coreFile"],
    ["releaseNotes", "releaseNotes"],
    ["releaseWorkflow", "releaseWorkflow"],
    ["proofWorkflow", "proofWorkflow"],
    ["privateProofWorkflow", "privateProofWorkflow"],
    ["appcast", "appcast"],
    ["channel", "channel"],
  ] as const) {
    if (typeof options[source] === "string") {
      out[target] = options[source];
    }
  }
  if (options.requireClean) {
    out.requireClean = true;
  }
  if (!out.channel) {
    const channel = argvOption("channel");
    if (channel) out.channel = channel;
  }
  return out;
}

function manifestPathOptions(manifestPath?: string): ManifestOptions {
  return manifestPath ? { manifest: manifestPath } : {};
}

function argvOption(name: string): string {
  const dashed = `--${name}`;
  for (let index = 2; index < process.argv.length; index += 1) {
    const arg = process.argv[index];
    if (arg === dashed) {
      const value = process.argv[index + 1];
      return value && !value.startsWith("--") ? value : "";
    }
    if (arg?.startsWith(`${dashed}=`)) {
      return arg.slice(dashed.length + 1);
    }
  }
  return "";
}

async function main(): Promise<void> {
  const program = new Command();
  program
    .name("release-train")
    .description("Manifest-driven 1Context release runner.")
    .option("--channel <name>", "release channel");

  addChannelOption(program.command("validate"))
    .description("Validate release manifest, policy, workflows, and channel gates.")
    .action(async function action(options: ChannelOptions) {
      await phaseValidate(contextFor(this, options));
    });

  addChannelOption(program.command("build"))
    .description("Build release artifacts for a channel.")
    .allowUnknownOption(false)
    .option("--dry-run", "run build preflight without producing artifacts")
    .action(async function action(options: ChannelOptions & { dryRun?: boolean }) {
      const args = options.dryRun ? ["--dry-run"] : [];
      await phaseBuild(contextFor(this, options), args);
    });

  addChannelOption(program.command("publish"))
    .description("Publish already-built release artifacts.")
    .action(async function action(options: ChannelOptions) {
      await phasePublish(contextFor(this, options));
    });

  addChannelOption(program.command("prove"))
    .description("Dispatch or execute release proof.")
    .allowUnknownOption(false)
    .option("--dry-run", "print the proof request without dispatching")
    .option("--dispatch", "dispatch proof workflow")
    .option("--runner-execute", "execute on the self-hosted proof runner")
    .option("--repo <repo>", "GitHub owner/repo")
    .option("--workflow <workflow>", "workflow file name")
    .option("--ref <ref>", "trusted ref to run")
    .option("--proof-reason <reason>", "human-readable proof reason")
    .action(async function action(options: ChannelOptions & Record<string, unknown>) {
      const args: string[] = [];
      for (const flag of ["dryRun", "dispatch", "runnerExecute"] as const) {
        if (options[flag]) {
          args.push(`--${flag.replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`)}`);
        }
      }
      for (const [key, flag] of [["repo", "--repo"], ["workflow", "--workflow"], ["ref", "--ref"], ["proofReason", "--proof-reason"]] as const) {
        if (typeof options[key] === "string") {
          args.push(flag, options[key]);
        }
      }
      await phaseProve(contextFor(this, options), args);
    });

  addChannelOption(program.command("audit"))
    .description("Audit published release assets and evidence.")
    .action(async function action(options: ChannelOptions) {
      await phaseAudit(contextFor(this, options));
    });

  addChannelOption(program.command("bless"))
    .description("Bless a release from existing evidence only.")
    .action(async function action(options: ChannelOptions) {
      await phaseBless(contextFor(this, options));
    });

  const manifest = program.command("manifest").description("Low-level manifest helpers for release tools.");
  addManifestCommonOptions(manifest.command("validate"))
    .action(async (options: Record<string, unknown>) => {
      const opts = manifestOptions(options);
      const paths = defaultManifestPaths(opts);
      await validateManifest(opts);
      console.log(`release manifest valid: ${paths.manifest}`);
    });
  addManifestCommonOptions(manifest.command("export-env"))
    .action(async (options: Record<string, unknown>) => {
      const opts = manifestOptions(options);
      const paths = defaultManifestPaths(opts);
      const releaseManifest = await validateManifest(opts);
      process.stdout.write(shellExport(envForManifest(releaseManifest, paths.manifest, opts.channel ?? "")));
    });
  manifest.command("check-clean-tree")
    .option("--root <path>", "repo root")
    .action(async (options: { root?: string }) => {
      await checkCleanTree(path.resolve(options.root ?? defaultManifestPaths().root));
      console.log("release tree is clean");
    });
  manifest.command("check-sourced-helpers")
    .option("--root <path>", "repo root")
    .action(async (options: { root?: string }) => {
      await checkSourcedHelpers(path.resolve(options.root ?? defaultManifestPaths().root));
      console.log("sourced shell helpers are tracked and tested");
    });
  addManifestCommonOptions(manifest.command("write-asset-manifest"))
    .option("--dist-dir <path>", "dist directory", "dist")
    .requiredOption("--output <path>", "output asset manifest path")
    .action(async (options: Record<string, unknown> & { distDir: string; output: string }) => {
      const opts = manifestOptions(options);
      const releaseManifest = await validateManifest(opts);
      await writeAssetManifest(releaseManifest, path.resolve(options.distDir), path.resolve(options.output));
      console.log(`wrote asset manifest: ${path.resolve(options.output)}`);
    });
  manifest.command("forbidden-patterns")
    .option("--manifest <path>", "release manifest path")
    .action((options: { manifest?: string }) => {
      const paths = defaultManifestPaths(manifestPathOptions(options.manifest));
      for (const pattern of forbiddenPatterns(loadManifestFile(paths.manifest))) {
        console.log(pattern);
      }
    });
  manifest.command("matrix-cases")
    .option("--manifest <path>", "release manifest path")
    .action((options: { manifest?: string }) => {
      const paths = defaultManifestPaths(manifestPathOptions(options.manifest));
      for (const matrixCase of matrixCases(loadManifestFile(paths.manifest))) {
        console.log(matrixCase);
      }
    });
  addManifestCommonOptions(manifest.command("write-fixture-proof-results"))
    .requiredOption("--output-dir <path>", "proof result output directory")
    .action(async (options: Record<string, unknown> & { outputDir: string }) => {
      const releaseManifest = await validateManifest(manifestOptions(options));
      for (const item of writeFixtureProofResults(releaseManifest, path.resolve(options.outputDir))) {
        console.log(item);
      }
    });

  await program.parseAsync(process.argv);
}

main().catch((error: unknown) => {
  if (error instanceof ReleaseError) {
    console.error(`release train failed: ${error.message}`);
    process.exitCode = 1;
    return;
  }
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exitCode = 1;
});

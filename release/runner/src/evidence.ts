import fs from "node:fs";
import path from "node:path";
import { ReleaseError } from "./errors.js";
import type { ReleaseContext } from "./context.js";

type StepAction<T> = () => Promise<T> | T;

export function ensureEvidenceDirs(ctx: ReleaseContext): void {
  fs.mkdirSync(ctx.evidenceDir, { recursive: true });
  fs.mkdirSync(ctx.proofResultsDir, { recursive: true });
}

export function writeJson(filePath: string, value: unknown): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, JSON.stringify(value, null, 2) + "\n", "utf8");
}

export function writeReleaseEvidence(ctx: ReleaseContext, phase: string): void {
  ensureEvidenceDirs(ctx);
  writeJson(ctx.releaseEvidence, {
    schema_version: "1context.release-evidence.v1",
    phase,
    version: ctx.version,
    previous_version: ctx.previousVersion,
    tag: ctx.tag,
    update_class: ctx.updateClass,
    public_appcast_url: ctx.publicAppcastUrl,
    generated_at: new Date().toISOString(),
    github_ref: process.env.GITHUB_REF ?? "",
    github_sha: process.env.GITHUB_SHA ?? "",
    required_files: {
      asset_manifest: "asset-manifest.json",
      redaction_report: "redaction-report.json",
      runner_attestation: "runner-attestation.json",
      timing_summary: "timing-summary.json",
      perception_db_ultra_max: "perception-db-ultra-max/*.json",
      proof_results: "proof-results/*.json",
    },
  });
}

function loadJson(filePath: string): Record<string, unknown> {
  return JSON.parse(fs.readFileSync(filePath, "utf8")) as Record<string, unknown>;
}

function publicRecord(record: Record<string, unknown>): Record<string, unknown> {
  const output: Record<string, unknown> = {};
  for (const key of [
    "_path",
    "stage",
    "step",
    "channel",
    "status",
    "elapsed_seconds",
    "budget_seconds",
    "budget_advisory",
    "budget_exceeded",
    "started_at",
    "ended_at",
  ]) {
    if (Object.prototype.hasOwnProperty.call(record, key)) {
      output[key] = record[key];
    }
  }
  return output;
}

export function writeTimingSummary(ctx: ReleaseContext): void {
  const timingDir = path.join(ctx.evidenceDir, "timings");
  if (!fs.existsSync(timingDir)) return;
  const stageRecords = fs.readdirSync(timingDir)
    .filter((name) => name.endsWith(".json") && name !== "summary.json")
    .sort()
    .map((name) => {
      const record = loadJson(path.join(timingDir, name));
      record._path = path.join("timings", name);
      return record;
    });
  const stepsDir = path.join(timingDir, "steps");
  const stepRecords = fs.existsSync(stepsDir)
    ? fs.readdirSync(stepsDir)
      .filter((name) => name.endsWith(".json"))
      .sort()
      .map((name) => {
        const record = loadJson(path.join(stepsDir, name));
        record._path = path.join("timings", "steps", name);
        return record;
      })
    : [];
  const allRecords = [...stageRecords, ...stepRecords];
  const budgetExceeded = allRecords.filter((record) => Boolean(record.budget_exceeded)).map(publicRecord);
  const failed = allRecords
    .filter((record) => !new Set(["passed", "dry-run"]).has(String(record.status ?? "").toLowerCase()))
    .map(publicRecord);
  const slowestSteps = stepRecords
    .map(publicRecord)
    .sort((left, right) => Number(right.elapsed_seconds ?? 0) - Number(left.elapsed_seconds ?? 0))
    .slice(0, 8);
  writeJson(path.join(ctx.evidenceDir, "timing-summary.json"), {
    schema_version: "1context.release-timing-summary.v1",
    version: ctx.version,
    channel: ctx.channel,
    generated_at: new Date().toISOString(),
    stage_count: stageRecords.length,
    step_count: stepRecords.length,
    stage_elapsed_seconds: stageRecords.reduce((sum, record) => sum + Number(record.elapsed_seconds ?? 0), 0),
    step_elapsed_seconds: stepRecords.reduce((sum, record) => sum + Number(record.elapsed_seconds ?? 0), 0),
    budget_exceeded_count: budgetExceeded.length,
    failed_count: failed.length,
    budget_exceeded: budgetExceeded,
    failed,
    stages: stageRecords.map(publicRecord),
    slowest_steps: slowestSteps,
  });
}

function slugify(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

export function writeStageTiming(ctx: ReleaseContext, stage: string, status: string, startedMs: number): void {
  ensureEvidenceDirs(ctx);
  const endedMs = Date.now();
  const elapsedSeconds = Math.floor((endedMs - startedMs) / 1000);
  const budgetKey = `budget_${stage}_seconds` as keyof typeof ctx.policy;
  const budgetSeconds = Number(ctx.policy[budgetKey] ?? 0);
  const advisory = ctx.policy.budget_is_advisory;
  writeJson(path.join(ctx.evidenceDir, "timings", `${stage}-${ctx.channel}.json`), {
    schema_version: ctx.manifest.release_factory.stage_timing_schema,
    stage,
    channel: ctx.channel,
    status,
    started_epoch: Math.floor(startedMs / 1000),
    ended_epoch: Math.floor(endedMs / 1000),
    started_at: new Date(startedMs).toISOString(),
    ended_at: new Date(endedMs).toISOString(),
    elapsed_seconds: elapsedSeconds,
    budget_seconds: budgetSeconds,
    budget_advisory: advisory,
    budget_exceeded: budgetSeconds > 0 && elapsedSeconds > budgetSeconds,
  });
  writeTimingSummary(ctx);
  if (!advisory && budgetSeconds > 0 && elapsedSeconds > budgetSeconds) {
    throw new ReleaseError(`${stage} exceeded ${budgetSeconds}s budget for ${ctx.channel} channel (${elapsedSeconds}s).`);
  }
}

export function writeStepTiming(ctx: ReleaseContext, stage: string, step: string, status: string, startedMs: number): void {
  ensureEvidenceDirs(ctx);
  const endedMs = Date.now();
  const elapsedSeconds = Math.floor((endedMs - startedMs) / 1000);
  writeJson(path.join(ctx.evidenceDir, "timings", "steps", `${stage}-${ctx.channel}-${slugify(step)}.json`), {
    schema_version: ctx.manifest.release_factory.stage_timing_schema,
    stage,
    step,
    channel: ctx.channel,
    status,
    started_epoch: Math.floor(startedMs / 1000),
    ended_epoch: Math.floor(endedMs / 1000),
    started_at: new Date(startedMs).toISOString(),
    ended_at: new Date(endedMs).toISOString(),
    elapsed_seconds: elapsedSeconds,
    budget_seconds: 0,
    budget_advisory: true,
    budget_exceeded: false,
  });
  writeTimingSummary(ctx);
}

export async function timeReleaseStep<T>(ctx: ReleaseContext, stage: string, step: string, action: StepAction<T>): Promise<T> {
  const startedMs = Date.now();
  try {
    const result = await action();
    writeStepTiming(ctx, stage, step, "passed", startedMs);
    return result;
  } catch (error) {
    writeStepTiming(ctx, stage, step, "failed", startedMs);
    throw error;
  }
}

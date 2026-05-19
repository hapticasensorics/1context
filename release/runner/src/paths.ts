import { fileURLToPath } from "node:url";
import path from "node:path";

const thisFile = fileURLToPath(import.meta.url);

export const runnerRoot = path.resolve(path.dirname(thisFile), "..");
export const repoRoot = path.resolve(runnerRoot, "..", "..");

export function fromRoot(...parts: string[]): string {
  return path.join(repoRoot, ...parts);
}

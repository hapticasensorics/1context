import { execa, type Options } from "execa";
import { ReleaseError } from "./errors.js";
import { repoRoot } from "./paths.js";

export async function commandExists(command: string): Promise<boolean> {
  const result = await execa("command", ["-v", command], { shell: true, reject: false });
  return result.exitCode === 0;
}

export async function requireTool(command: string): Promise<void> {
  if (!(await commandExists(command))) {
    throw new ReleaseError(`Missing required tool: ${command}`);
  }
}

export async function runCommand(command: string, args: string[] = [], options: Options = {}): Promise<void> {
  const stdioOptions = options as Options & { stdin?: unknown; stdout?: unknown; stderr?: unknown; stdio?: unknown };
  const useInheritedStdio =
    stdioOptions.stdio === undefined &&
    stdioOptions.stdin === undefined &&
    stdioOptions.stdout === undefined &&
    stdioOptions.stderr === undefined;
  const base = useInheritedStdio
    ? { cwd: repoRoot, env: process.env, stdio: "inherit" as const }
    : { cwd: repoRoot, env: process.env };
  const executable = command.endsWith(".sh") ? "bash" : command;
  const executableArgs = command.endsWith(".sh") ? [command, ...args] : args;
  await execa(executable, executableArgs, {
    ...base,
    ...options,
  });
}

export async function runCapture(command: string, args: string[] = [], options: Options = {}): Promise<string> {
  const result = await execa(command, args, {
    cwd: repoRoot,
    env: process.env,
    ...options,
  });
  return String(result.stdout ?? "");
}

export async function runShell(script: string, options: Options = {}): Promise<void> {
  await execa("bash", ["-c", script], {
    cwd: repoRoot,
    stdio: "inherit",
    env: process.env,
    ...options,
  });
}

export async function runShellCapture(script: string, options: Options = {}): Promise<string> {
  const result = await execa("bash", ["-c", script], {
    cwd: repoRoot,
    env: process.env,
    ...options,
  });
  return String(result.stdout ?? "");
}

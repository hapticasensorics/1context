export class ReleaseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ReleaseError";
  }
}

export function assertRelease(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new ReleaseError(message);
  }
}

import Foundation

/// Generates the zsh script that the menu bar's "Update 1Context…" command
/// hands off to a Terminal window.
///
/// The script's job is to run `1context-cli update` while keeping the user
/// fully informed about success or failure, even when Terminal is configured
/// to close its window on shell exit (a common preference that hid the
/// outcome from earlier external users).
///
/// The script:
/// * mirrors all output to a per-run log file under the runtime log directory
///   so users (and support) can inspect what happened even after the window
///   closes;
/// * waits for the user to press Return before exiting so they have time to
///   read the result;
/// * shows a native dialog on success and on failure;
/// * propagates the CLI's exit status as its own.
///
/// The script self-deletes from the temporary directory via an EXIT trap.
/// Log files are persistent so they remain available for diagnosis.
public enum UpdaterScript {
  /// Builds the zsh source for the updater hand-off script.
  ///
  /// - Parameters:
  ///   - cliExecutable: Absolute path to the `1context-cli` binary.
  ///   - alertExecutable: Absolute path to the menu-bar binary that supports
  ///     the `--update-success-alert` flag (a fish alert). The script falls
  ///     back to `osascript` if this binary is missing or fails.
  ///   - logDirectory: Directory under which the per-run `update-<UTC>.log`
  ///     file will be written. Created with `mkdir -p` if missing.
  /// - Returns: The script source as a UTF-8 string. Callers are responsible
  ///   for writing it to a file under the temporary directory and making it
  ///   executable.
  public static func render(
    cliExecutable: String,
    alertExecutable: String,
    logDirectory: String
  ) -> String {
    let cli = shellQuote(cliExecutable)
    let alert = shellQuote(alertExecutable)
    let logDir = shellQuote(logDirectory)

    return """
    #!/bin/zsh
    set -uo pipefail
    trap 'rm -f "$0"' EXIT

    LOG_DIR=\(logDir)
    mkdir -p "$LOG_DIR" 2>/dev/null || true
    LOG_FILE="$LOG_DIR/update-$(/bin/date -u +%Y%m%dT%H%M%SZ).log"

    # Mirror all output to a log file so the result survives Terminal's
    # "close window on shell exit" preference. Without this, a window that
    # auto-closes leaves the user with no record of what happened.
    exec > >(tee -a "$LOG_FILE") 2>&1

    printf '%s\\n' 'Updating 1Context...'
    printf '%s\\n' "Log: $LOG_FILE"
    printf '%s\\n\\n' 'If prompted, enter your Mac password. Terminal hides characters as you type.'

    # Note: avoid the bare name `status` — zsh reserves it as a read-only
    # alias for $?, and assigning to it would abort the script.
    \(cli) update
    cli_status=$?

    if [ "$cli_status" -eq 0 ]; then
      \(alert) --update-success-alert >/dev/null 2>&1 \\
        || osascript -e 'display dialog "1Context updated." buttons {"OK"} default button "OK"' >/dev/null 2>&1 \\
        || true
      printf '\\n%s\\n' 'Done.'
    else
      osascript -e 'display dialog "1Context update failed. See Terminal window for details." buttons {"OK"} default button "OK" with icon caution' >/dev/null 2>&1 || true
      printf '\\n%s\\n' 'Update failed.'
      printf '%s\\n' "Details: $LOG_FILE"
    fi

    # Hold the window open so the user can read the outcome even when
    # Terminal is configured to close on shell exit. EOF on stdin (e.g. in
    # tests) returns immediately so this is safe in non-interactive contexts.
    printf '\\n%s ' 'Press Return to close.'
    IFS= read -r _ || true
    exit "$cli_status"
    """
  }

  /// POSIX-safe single-quote shell escape. Wraps `value` in single quotes and
  /// escapes any embedded single quotes using the standard `'\''` idiom.
  static func shellQuote(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }
}

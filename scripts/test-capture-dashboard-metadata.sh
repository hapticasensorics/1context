#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage:
  scripts/test-capture-dashboard-metadata.sh

Environment:
  ONECONTEXT_APP                                  Installed app bundle to query.
                                                  Defaults to /Applications/1Context Dev.app.
  ONECONTEXT_CAPTURE_CLI                         Override the 1context CLI path.
  ONECONTEXT_CAPTURE_DASHBOARD_EVIDENCE_DIR      Defaults to
                                                  dist/capture-dashboard-metadata-evidence/<timestamp>.

Captures capture.status and capture.snapshot from the installed app CLI, writes
the raw payloads plus a jq-derived metadata summary, and verifies the fields the
Rust capture dashboard depends on when Accessibility and Input Monitoring are
granted. This does not require Chrome or any specific foreground application.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

fail() {
  echo "capture dashboard metadata validation failed: $*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "required tool not found: $1"
}

jq_expect() {
  local label="$1"
  local file="$2"
  shift 2
  if ! jq -e "$@" "$file" >/dev/null; then
    echo "Failed expectation: $label" >&2
    echo "Filter: $*" >&2
    echo "Payload: $file" >&2
    exit 1
  fi
}

require_tool jq
require_tool /usr/libexec/PlistBuddy

APP="${ONECONTEXT_APP:-/Applications/1Context Dev.app}"
APP="${APP%/}"
INFO="$APP/Contents/Info.plist"
CLI="${ONECONTEXT_CAPTURE_CLI:-$APP/Contents/MacOS/1context-cli}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_CAPTURE_DASHBOARD_EVIDENCE_DIR:-$ROOT/dist/capture-dashboard-metadata-evidence/$STAMP}"
STATUS_JSON="$EVIDENCE_DIR/capture.status.json"
SNAPSHOT_JSON="$EVIDENCE_DIR/capture.snapshot.json"
SUMMARY_JSON="$EVIDENCE_DIR/dashboard-metadata-summary.json"

[[ -d "$APP" ]] || fail "app bundle not found: $APP"
[[ -f "$INFO" ]] || fail "Info.plist not found: $INFO"
[[ -x "$CLI" ]] || fail "1context CLI not found or not executable: $CLI"

mkdir -p "$EVIDENCE_DIR"

BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$INFO")"
APP_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$INFO")"
printf '%s\n' "$BUNDLE_ID" >"$EVIDENCE_DIR/bundle-identifier.txt"
printf '%s\n' "$APP_VERSION" >"$EVIDENCE_DIR/version.txt"

"$CLI" capture status >"$STATUS_JSON"
"$CLI" capture snapshot >"$SNAPSHOT_JSON"

jq_expect "capture.status surface" "$STATUS_JSON" '.surface == "capture_status"'
jq_expect "capture.status exposes dashboard RPCs" "$STATUS_JSON" '
  (.available_methods | index("capture.status")) and
  (.available_methods | index("capture.snapshot"))
'
jq_expect "capture.status reports motion hint fusion" "$STATUS_JSON" '
  .metadata_sample_fusion.pixels_untouched == true and
  .metadata_sample_fusion.ux_motion_hints_enabled == true and
  .metadata_sample_fusion.source == "persistent_ux_event_tap"
'
jq_expect "capture.status exposes structured permission-derived metadata" "$STATUS_JSON" '
  .permission_derived_metadata.schema_version == 1 and
  (.permission_derived_metadata.generated_at | type == "string") and
  .permission_derived_metadata.privacy.raw_keystrokes_included == false and
  .permission_derived_metadata.privacy.raw_text_included == false and
  .permission_derived_metadata.privacy.coordinates_included == false and
  .permission_derived_metadata.privacy.aggregates_and_counts_only == true and
  (.permission_derived_metadata.process_identities | length) >= 2 and
  (.permission_derived_metadata.capture_paths.events_directory | type == "string") and
  (.permission_derived_metadata.signals.accessibility.ready | type == "boolean") and
  .permission_derived_metadata.signals.accessibility.focused_context.trusted == true and
  .permission_derived_metadata.signals.input_monitoring.event_tap.active == true and
  (.permission_derived_metadata.signals.input_monitoring.event_tap.observed_event_count | type == "number")
'
jq_expect "capture.status has typed motion hints" "$STATUS_JSON" '
  .motion_hints.generatedAt | type == "string"
'
jq_expect "capture.status has typed motion booleans" "$STATUS_JSON" '
  (.motion_hints.focusedRecently | type == "boolean") and
  (.motion_hints.keyboardActivityRecently | type == "boolean") and
  (.motion_hints.scrollEventRecently | type == "boolean") and
  (.motion_hints.estimatedScrollDY | type == "number")
'
jq_expect "Input Monitoring event tap is active" "$STATUS_JSON" '
  .ux_event_tap.startup_wired == true and
  .ux_event_tap.tap_active == true and
  .ux_event_tap.lifecycle_state == "running" and
  .ux_event_tap.metadata_sample_fusion_enabled == true
'
jq_expect "Input Monitoring permission subject matches installed app" "$STATUS_JSON" --arg bundle_id "$BUNDLE_ID" '
  .ux_event_tap.permission_subject.bundleIdentifier == $bundle_id and
  (.ux_event_tap.permission_subject.designatedRequirementSHA256 | type == "string") and
  (.ux_event_tap.permission_subject.tap_owner_pid | type == "number") and
  (.ux_event_tap.permission_subject.tap_owner_bundle_identifier // .ux_event_tap.tap_owner_bundle) == $bundle_id
'

jq_expect "capture.snapshot surface" "$SNAPSHOT_JSON" '.surface == "capture_window_snapshot"'
jq_expect "capture.snapshot embeds status metadata" "$SNAPSHOT_JSON" '
  .capture_status.surface == "capture_status" and
  .capture_status.metadata_sample_fusion.ux_motion_hints_enabled == true
'
jq_expect "Accessibility focused context is trusted" "$SNAPSHOT_JSON" '
  .focusedContext.isProcessTrusted == true and
  .focusedContext.status != "not_trusted" and
  (.focusedContext.activeApplication.bundleID | type == "string" and length > 0)
'
jq_expect "snapshot exposes focused window metadata for dashboard" "$SNAPSHOT_JSON" '
  ([.windows[] | select(.isFocused == true)] | length) >= 1 and
  ([.windows[] | select(.focusMetadata != null)] | length) >= 1
'

jq -n \
  --arg schema "1context.capture-dashboard-metadata-validation.v1" \
  --arg app "$APP" \
  --arg cli "$CLI" \
  --arg bundle_id "$BUNDLE_ID" \
  --arg app_version "$APP_VERSION" \
  --slurpfile status "$STATUS_JSON" \
  --slurpfile snapshot "$SNAPSHOT_JSON" '
    ($status[0]) as $status |
    ($snapshot[0]) as $snapshot |
    {
      schema: $schema,
      status: "passed",
      app: $app,
      cli: $cli,
      bundle_identifier: $bundle_id,
      app_version: $app_version,
      captured_payloads: {
        status: "capture.status.json",
        snapshot: "capture.snapshot.json"
      },
      expectations_when_granted: {
        accessibility: [
          "focusedContext.isProcessTrusted == true",
          "focusedContext.status != not_trusted",
          "focusedContext.activeApplication.bundleID is non-empty",
          "at least one snapshot window has focusMetadata"
        ],
        input_monitoring: [
          "ux_event_tap.startup_wired == true",
          "ux_event_tap.tap_active == true",
          "ux_event_tap.lifecycle_state == running",
          "ux_event_tap.permission_subject matches the installed app bundle id",
          "permission_derived_metadata.signals.input_monitoring.event_tap.active == true"
        ],
        motion_hints: [
          "metadata_sample_fusion.ux_motion_hints_enabled == true",
          "motion_hints fields are typed and present",
          "pixels_untouched == true"
        ]
      },
      permission_subject: {
        bundle_identifier: $status.ux_event_tap.permission_subject.bundleIdentifier,
        executable_path: $status.ux_event_tap.permission_subject.executablePath,
        designated_requirement_sha256: $status.ux_event_tap.permission_subject.designatedRequirementSHA256,
        tap_owner_process: $status.ux_event_tap.permission_subject.tap_owner_process,
        tap_owner_pid: $status.ux_event_tap.permission_subject.tap_owner_pid,
        tap_owner_bundle_identifier: (
          $status.ux_event_tap.permission_subject.tap_owner_bundle_identifier
          // $status.ux_event_tap.tap_owner_bundle
        )
      },
      permission_derived_metadata: {
        schema_version: $status.permission_derived_metadata.schema_version,
        generated_at: $status.permission_derived_metadata.generated_at,
        privacy: $status.permission_derived_metadata.privacy,
        process_identities: $status.permission_derived_metadata.process_identities,
        capture_paths: $status.permission_derived_metadata.capture_paths,
        signals: {
          accessibility: $status.permission_derived_metadata.signals.accessibility,
          input_monitoring: $status.permission_derived_metadata.signals.input_monitoring,
          screen_capture: $status.permission_derived_metadata.signals.screen_capture,
          system_audio: $status.permission_derived_metadata.signals.system_audio
        }
      },
      input_monitoring: {
        startup_wired: $status.ux_event_tap.startup_wired,
        tap_active: $status.ux_event_tap.tap_active,
        lifecycle_state: $status.ux_event_tap.lifecycle_state,
        event_tap: $status.ux_event_tap.event_tap,
        tap_options: $status.ux_event_tap.tap_options,
        observed_event_count: $status.ux_event_tap.observed_event_count,
        dropped_count: $status.ux_event_tap.dropped_count,
        queue_depth: $status.ux_event_tap.queue_depth,
        jsonl_persistence: $status.ux_event_tap.jsonl_persistence
      },
      motion_hints: {
        status: $status.motion_hints,
        ux_event_tap: $status.ux_event_tap.motion_hints,
        snapshot_capture_status: $snapshot.capture_status.motion_hints,
        metadata_sample_fusion: $status.metadata_sample_fusion
      },
      focus: {
        active_application: $snapshot.activeApplication,
        focused_context_status: $snapshot.focusedContext.status,
        focused_context_trusted: $snapshot.focusedContext.isProcessTrusted,
        focused_application_process_id: $snapshot.focusedContext.focusedApplicationProcessID,
        matched_window_id: $snapshot.focusedContext.matchedWindowID,
        focused_window_count: ([$snapshot.windows[] | select(.isFocused == true)] | length),
        focus_metadata_count: ([$snapshot.windows[] | select(.focusMetadata != null)] | length),
        focused_windows: [
          $snapshot.windows[]
          | select(.isFocused == true)
          | {
              appName,
              bundleID,
              title,
              windowID,
              focusMetadata
            }
        ]
      }
    }
  ' >"$SUMMARY_JSON"

echo "Capture dashboard metadata validation passed."
echo "Evidence: $SUMMARY_JSON"

# 1Context Agent Instructions

Follow the global Codex instructions plus these repo-specific rules.

## Schema And Compatibility Policy

Do not add migration systems, backwards-compatibility scaffolds, legacy upgrade
paths, or compatibility tables unless the user explicitly asks for them. This
project currently prefers deleting the old implementation first, then replacing
it with a cleaner solution when the right design is clear. If a deleted system
is still referenced by builds or tests, remove or no-op those references rather
than recreating stale migration files.

## Dev Builds And Permission Testing

Use the stable dev build for ordinary iteration:

```bash
./scripts/release-train.sh build --channel dev
ditto --norsrc --noqtn "dist/1Context Dev.app" "/Applications/1Context Dev.app"
open -na "/Applications/1Context Dev.app"
"/Applications/1Context Dev.app/Contents/MacOS/1context-cli" diagnose
```

Use a timestamped dev build only when the task is specifically about fresh macOS
TCC permission prompts or first-run setup. Keep the timestamp in a variable and
use it consistently for build, install, diagnostics, and probe evidence:

```bash
BUILD_TIME="$(date +%Y%m%d-%H%M%S)"
/usr/bin/time -p env ONECONTEXT_PERMISSION_TEST_ID="$BUILD_TIME" \
  ./scripts/release-train.sh build --channel dev

APP_NAME="1Context Dev - $BUILD_TIME"
ditto --norsrc --noqtn "dist/$APP_NAME.app" "/Applications/$APP_NAME.app"
open -na "/Applications/$APP_NAME.app"
"/Applications/$APP_NAME.app/Contents/MacOS/1context-cli" diagnose

ONECONTEXT_APP="/Applications/$APP_NAME.app" \
ONECONTEXT_INCLUDE_BROWSER_EXTENSION=1 \
./scripts/test-installed-app-live-permission-capabilities.sh
```

Timestamped builds produce bundle identifiers like
`com.haptica.1context.dev.permission.<build-time>` and app names like
`1Context Dev - <build-time>`. They intentionally get fresh TCC identities.
Do not use timestamped builds to judge whether normal dev rebuilds preserve
already-granted permissions.

When reporting a dev build result, include:

- The `BUILD_TIME` value.
- The installed app path under `/Applications`.
- The `real/user/sys` timing from `/usr/bin/time -p`.
- The evidence path from the live permission capability probe when permissions
  are part of the task.

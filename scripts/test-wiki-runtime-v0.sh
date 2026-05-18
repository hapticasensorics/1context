#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="$(mktemp -d /tmp/1ctx-wiki-runtime-smoke-XXXXXX)"
FIXTURE_HOME="$(mktemp -d /tmp/1ctx-dev-user-data-fixture-XXXXXX)"

cleanup() {
  rm -rf "$RUNTIME_TEST"
  rm -rf "$FIXTURE_HOME"
}
trap cleanup EXIT

FIXTURE_PAGE="$FIXTURE_HOME/1Context/user-wiki/source/families/zz-smoke/runtime-smoke"
mkdir -p \
  "$FIXTURE_PAGE/source" \
  "$FIXTURE_PAGE/talk/runtime-smoke.talk" \
  "$FIXTURE_PAGE/templates/talk" \
  "$FIXTURE_HOME/1Context/context-engine/observations/smoke/runtime-smoke"

printf 'title = "Smoke Fixtures"\n' > "$FIXTURE_HOME/1Context/user-wiki/source/families/zz-smoke/group.toml"
printf 'title = "Runtime Smoke"\n' > "$FIXTURE_PAGE/family.toml"
printf '# Runtime Smoke\n\nFixture page.\n' > "$FIXTURE_PAGE/source/runtime-smoke.md"
printf 'title: Talk - Runtime Smoke\n' > "$FIXTURE_PAGE/talk/runtime-smoke.talk/_meta.yaml"
printf '# Talk conventions\n' > "$FIXTURE_PAGE/talk/runtime-smoke.talk/_conventions.md"
printf '# Curator\n' > "$FIXTURE_PAGE/talk/runtime-smoke.talk/_curator.md"
printf '# Page template\n' > "$FIXTURE_PAGE/templates/page.template.md"
printf '# Curator template\n' > "$FIXTURE_PAGE/templates/talk/_curator.template.md"
printf '{"event":"smoke"}\n' \
  > "$FIXTURE_HOME/1Context/context-engine/observations/smoke/runtime-smoke/events.jsonl"

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" "$FIXTURE_HOME" >/tmp/1ctx-wiki-runtime-v0.out
printf '# Runtime Smoke\n\nFixture page.\n\nLocal smoke edit: preserve me.\n' \
  > "$RUNTIME_TEST/1Context/user-wiki/source/families/zz-smoke/runtime-smoke/source/runtime-smoke.md"
printf '\nOperator edit: preserve this configured page.\n' \
  >> "$RUNTIME_TEST/1Context/user-wiki/source/families/context/your-context/source/your-context.md"
rm -f "$RUNTIME_TEST/1Context/user-wiki/source/families/reference/topics/source/topics.md"
printf 'reason = "smoke tombstone"\n' \
  > "$RUNTIME_TEST/1Context/user-wiki/source/families/reference/topics/source/topics.tombstone.toml"
"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" "$FIXTURE_HOME" >/tmp/1ctx-wiki-runtime-v0-second.out

test -d "$RUNTIME_TEST/1Context/user-wiki/source"
test -d "$RUNTIME_TEST/1Context/user-wiki/site/.1context"
test -d "$RUNTIME_TEST/1Context/context-engine/agents/roles"
test -d "$RUNTIME_TEST/1Context/context-engine/agents/tools"
test -d "$RUNTIME_TEST/1Context/context-engine/agents/policies"
test -d "$RUNTIME_TEST/1Context/context-engine/prompts/shared"
test -d "$RUNTIME_TEST/1Context/context-engine/indexes"
test -d "$RUNTIME_TEST/Library/Application Support/1Context/wiki-site/current"
test -d "$RUNTIME_TEST/Library/Application Support/1Context/indexes/lancedb"
test -d "$RUNTIME_TEST/Library/Application Support/1Context/setup"
test -d "$RUNTIME_TEST/Library/Logs/1Context"
test -d "$RUNTIME_TEST/Library/Caches/1Context"
test -f "$RUNTIME_TEST/1Context/user-wiki/README.md"
test -f "$RUNTIME_TEST/1Context/user-wiki/wiki.toml"
test -f "$RUNTIME_TEST/1Context/user-wiki/templates/pages/context-page.md"
test -f "$RUNTIME_TEST/1Context/user-wiki/templates/talk/conventions.md"
test -f "$RUNTIME_TEST/1Context/user-wiki/templates/site/home.md"
test -f "$RUNTIME_TEST/1Context/context-engine/prompts/e08-for-you/agent-profile.md"
test -f "$RUNTIME_TEST/1Context/context-engine/prompts/e08-for-you/librarian.md"
test -f "$RUNTIME_TEST/1Context/user-wiki/templates/talk/conventions/your-context.md"
test -f "$RUNTIME_TEST/1Context/user-wiki/templates/talk/curators/your-context.md"
test -f "$RUNTIME_TEST/1Context/user-wiki/templates/pages/e08/your-context.md"
test -f "$RUNTIME_TEST/1Context/user-wiki/templates/site/e08/index.md"

YOUR_CONTEXT_PAGE="$RUNTIME_TEST/1Context/user-wiki/source/families/context/your-context"
PROJECTS_PAGE="$RUNTIME_TEST/1Context/user-wiki/source/families/work/projects"
TOPICS_PAGE="$RUNTIME_TEST/1Context/user-wiki/source/families/reference/topics"

test -f "$YOUR_CONTEXT_PAGE/family.toml"
test -f "$YOUR_CONTEXT_PAGE/source/your-context.md"
test -f "$YOUR_CONTEXT_PAGE/talk/your-context.talk/_meta.yaml"
test -f "$YOUR_CONTEXT_PAGE/talk/your-context.talk/_conventions.md"
test -f "$YOUR_CONTEXT_PAGE/talk/your-context.talk/_curator.md"
test -f "$YOUR_CONTEXT_PAGE/templates/page.template.md"
test -f "$YOUR_CONTEXT_PAGE/templates/talk/entry.template.md"
test -f "$PROJECTS_PAGE/source/projects.md"
test -f "$PROJECTS_PAGE/talk/projects.talk/_meta.yaml"
test -f "$TOPICS_PAGE/source/topics.tombstone.toml"
test ! -f "$TOPICS_PAGE/source/topics.md"
grep -q 'Operator edit: preserve this configured page.' "$YOUR_CONTEXT_PAGE/source/your-context.md"

SMOKE_PAGE="$RUNTIME_TEST/1Context/user-wiki/source/families/zz-smoke/runtime-smoke"
test -f "$SMOKE_PAGE/source/runtime-smoke.md"
test -f "$SMOKE_PAGE/talk/runtime-smoke.talk/_meta.yaml"
test -f "$SMOKE_PAGE/talk/runtime-smoke.talk/_curator.md"
test -f "$SMOKE_PAGE/talk/runtime-smoke.talk/_conventions.md"
test -f "$SMOKE_PAGE/templates/page.template.md"
test -f "$SMOKE_PAGE/templates/talk/_curator.template.md"
test -f "$RUNTIME_TEST/1Context/context-engine/observations/smoke/runtime-smoke/events.jsonl"
grep -q 'Local smoke edit: preserve me.' "$SMOKE_PAGE/source/runtime-smoke.md"

INSTALL_STATE="$RUNTIME_TEST/Library/Application Support/1Context/setup/dev-user-data-import.toml"
MATERIALIZE_STATE="$RUNTIME_TEST/Library/Application Support/1Context/setup/wiki-page-materialize.toml"
test -f "$INSTALL_STATE"
test -f "$MATERIALIZE_STATE"

grep -q '^source_root = "' "$INSTALL_STATE"
grep -q "1Context/user-wiki/source/families/zz-smoke/runtime-smoke/source/runtime-smoke.md" \
  "$INSTALL_STATE"
grep -q "1Context/context-engine/observations/smoke/runtime-smoke/events.jsonl" \
  "$INSTALL_STATE"

if ! grep -q 'status = "skipped_modified"' "$INSTALL_STATE"; then
  echo "second smoke import should preserve edited runtime-test files" >&2
  exit 1
fi

grep -q 'path = "1Context/user-wiki/source/families/context/your-context/source/your-context.md"' "$MATERIALIZE_STATE"
grep -q 'status = "skipped_existing"' "$MATERIALIZE_STATE"
grep -q 'status = "tombstoned"' "$MATERIALIZE_STATE"

for ignored_path in \
  "runtime-test/1Context/user-wiki/source/families/sample-group/sample-page/source/private.md" \
  "runtime/local-user-data/1Context/user-wiki/source/families/sample-group/sample-page/source/private.md" \
  "runtime/fixtures/1Context/user-wiki/source/families/sample-group/sample-page/talk/private.talk/_meta.yaml" \
  "runtime/imports/1Context/user-wiki/site/index.html" \
  "runtime/probe/1Context/context-engine/runs/run.jsonl" \
  "runtime/probe/Library/Application Support/1Context/wiki-site/current/index.html"
do
  if ! git -C "$ROOT" check-ignore -q "$ignored_path"; then
    echo "expected runtime user-data path to be ignored: $ignored_path" >&2
    exit 1
  fi
done

for trackable_path in \
  "runtime/1Context/user-wiki/source/families/sample-group/sample-page/source/public.md" \
  "runtime/1Context/user-wiki/source/families/sample-group/sample-page/talk/public.talk/_meta.yaml" \
  "runtime/1Context/user-wiki/site/index.html" \
  "runtime/1Context/user-wiki/templates/pages/context-page.md" \
  "runtime/1Context/user-wiki/templates/talk/entry.md" \
  "runtime/1Context/user-wiki/templates/site/home.md" \
  "runtime/1Context/context-engine/indexes/index-manifest.toml" \
  "runtime/1Context/context-engine/prompts/e08-for-you/for-you-curator.md" \
  "runtime/1Context/user-wiki/templates/talk/curators/your-context.md"
do
  if git -C "$ROOT" check-ignore -q "$trackable_path"; then
    echo "expected reusable runtime path to remain trackable: $trackable_path" >&2
    exit 1
  fi
done

echo "wiki runtime v0 runtime-test smoke passed."

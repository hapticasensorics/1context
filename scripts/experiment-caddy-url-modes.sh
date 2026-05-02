#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK_DIR="$(mktemp -d /tmp/1ctx-caddy-url-modes-XXXXXX)"
RUN_LIVE=0
RUN_PRIVILEGED=0
SHOW_TRUST=0
HARNESS_HTTP_PORT="${ONECONTEXT_CADDY_HARNESS_HTTP_PORT:-39191}"
PROBE_HTTP_PORT="${ONECONTEXT_CADDY_PROBE_HTTP_PORT:-39193}"
PROBE_HTTPS_PORT="${ONECONTEXT_CADDY_PROBE_HTTPS_PORT:-39194}"

usage() {
  cat <<'USAGE'
Usage: scripts/experiment-caddy-url-modes.sh [--live] [--privileged] [--trust-instructions]

Default:
  Write candidate Caddy configs and validate syntax only.

Options:
  --live
      Start unprivileged high-port HTTP and HTTPS probes, then curl health.

  --privileged
      Attempt port 80 and 443 probes without sudo. This is safe to run:
      it records whether the current user can bind those ports, and it
      does not request elevation or install trust.

  --trust-instructions
      Print the explicit manual commands that would be needed to test local
      certificate trust in a disposable Caddy home.

Environment:
  ONECONTEXT_CADDY_PATH=/path/to/caddy
      Use a specific Caddy binary.
USAGE
}

while (($#)); do
  case "$1" in
    --live)
      RUN_LIVE=1
      ;;
    --privileged)
      RUN_PRIVILEGED=1
      ;;
    --trust-instructions)
      SHOW_TRUST=1
      ;;
    --all)
      RUN_LIVE=1
      RUN_PRIVILEGED=1
      SHOW_TRUST=1
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

cleanup() {
  if [[ -n "${CADDY_PID:-}" ]]; then
    kill "$CADDY_PID" >/dev/null 2>&1 || true
    wait "$CADDY_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

find_caddy() {
  if [[ -n "${ONECONTEXT_CADDY_PATH:-}" && -x "${ONECONTEXT_CADDY_PATH:-}" ]]; then
    printf '%s\n' "$ONECONTEXT_CADDY_PATH"
    return
  fi
  if [[ -x "$ROOT/dist/1Context.app/Contents/Resources/local-web/caddy/caddy" ]]; then
    printf '%s\n' "$ROOT/dist/1Context.app/Contents/Resources/local-web/caddy/caddy"
    return
  fi
  if command -v caddy >/dev/null 2>&1; then
    command -v caddy
    return
  fi
}

write_config() {
  local name="$1"
  local body="$2"
  local path="$WORK_DIR/$name.Caddyfile"
  printf '%s\n' "$body" > "$path"
  printf '%s\n' "$path"
}

validate_config() {
  local caddy="$1"
  local label="$2"
  local path="$3"

  if caddy_env "$caddy" validate --config "$path" >/dev/null 2>"$WORK_DIR/$label.validate.log"; then
    printf '  Validate: pass\n'
  else
    printf '  Validate: fail\n'
    sed 's/^/    /' "$WORK_DIR/$label.validate.log"
  fi
}

caddy_env() {
  HOME="$WORK_DIR/home" \
    XDG_DATA_HOME="$WORK_DIR/xdg-data" \
    XDG_CONFIG_HOME="$WORK_DIR/xdg-config" \
    "$@"
}

wait_for_probe() {
  local url="$1"
  local curl_args="$2"

  for _ in {1..40}; do
    # shellcheck disable=SC2086
    if curl --fail --silent --max-time 1 $curl_args "$url" >/dev/null 2>&1; then
      return 0
    fi
    if [[ -n "${CADDY_PID:-}" ]] && ! kill -0 "$CADDY_PID" >/dev/null 2>&1; then
      return 1
    fi
    sleep 0.1
  done
  return 1
}

run_probe() {
  local caddy="$1"
  local label="$2"
  local path="$3"
  local url="$4"
  local curl_args="${5:-}"
  local log="$WORK_DIR/$label.run.log"

  unset CADDY_PID
  caddy_env "$caddy" run --config "$path" >"$log" 2>&1 &
  CADDY_PID=$!

  if wait_for_probe "$url" "$curl_args"; then
    printf '  Live Probe: pass\n'
  else
    printf '  Live Probe: fail\n'
    sed 's/^/    /' "$log" | tail -n 20
  fi

  kill "$CADDY_PID" >/dev/null 2>&1 || true
  wait "$CADDY_PID" >/dev/null 2>&1 || true
  unset CADDY_PID
}

caddy="$(find_caddy || true)"

cat <<REPORT
1Context Caddy URL Mode Experiment

This harness keeps Caddy state in a temporary HOME/XDG directory:
  $WORK_DIR

Default mode validates config syntax only. --live starts high-port probes.
  --privileged attempts port 80/443 without sudo and records what happens.
It disables Caddy's automatic local-trust installation and never asks for
elevation.
REPORT

if [[ -z "$caddy" ]]; then
  cat <<'REPORT'

Caddy: missing

Install Caddy or build/package 1Context first:
  ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh
REPORT
  exit 0
fi

printf '\nCaddy: %s\n' "$caddy"

harness_config="$(write_config harness-high-port "{
  admin off
  auto_https off
}

http://wiki.1context.localhost:$HARNESS_HTTP_PORT, http://127.0.0.1:$HARNESS_HTTP_PORT {
  bind 127.0.0.1
  respond /__1context/health \"ok\"
  respond \"harness high-port localhost mode\"
}
")"

http_probe_config="$(write_config high-port-http-probe "{
  admin off
  auto_https off
}

http://wiki.1context.localhost:$PROBE_HTTP_PORT, http://127.0.0.1:$PROBE_HTTP_PORT {
  bind 127.0.0.1
  respond /__1context/health \"ok\"
  respond \"high-port http probe\"
}
")"

portless_config="$(write_config portless-http '{
  admin off
  auto_https off
}

http://wiki.1context.localhost, http://127.0.0.1 {
  bind 127.0.0.1
  respond /__1context/health "ok"
  respond "portless localhost http mode"
}
')"

https_probe_config="$(write_config high-port-https-probe "{
  admin off
  skip_install_trust
  auto_https disable_redirects
}

https://wiki.1context.localhost:$PROBE_HTTPS_PORT {
  bind 127.0.0.1
  tls internal
  respond /__1context/health \"ok\"
  respond \"high-port https probe\"
}
")"

https_config="$(write_config local-https '{
  admin off
  skip_install_trust
  auto_https disable_redirects
}

https://wiki.1context.localhost {
  bind 127.0.0.1
  tls internal
  respond /__1context/health "ok"
  respond "local https mode"
}
')"

cat <<REPORT

Mode: harness-high-port
  URL: http://wiki.1context.localhost:$HARNESS_HTTP_PORT/your-context
  Expected permission/trust: none
  Tradeoff: visible port
REPORT
validate_config "$caddy" harness-high-port "$harness_config"

cat <<REPORT

Mode: high-port-http-probe
  URL: http://wiki.1context.localhost:$PROBE_HTTP_PORT/your-context
  Expected permission/trust: none
  Purpose: live proof for the no-permission URL class
REPORT
validate_config "$caddy" high-port-http-probe "$http_probe_config"
if [[ "$RUN_LIVE" == "1" ]]; then
  run_probe "$caddy" high-port-http-probe "$http_probe_config" \
    "http://127.0.0.1:$PROBE_HTTP_PORT/__1context/health" \
    "-H Host:wiki.1context.localhost"
fi

cat <<REPORT

Mode: portless-http
  URL: http://wiki.1context.localhost/your-context
  Expected permission/trust: privileged bind for port 80 or helper/proxy
  Tradeoff: cleaner URL, more installer/lifecycle complexity
REPORT
validate_config "$caddy" portless-http "$portless_config"
if [[ "$RUN_PRIVILEGED" == "1" ]]; then
  run_probe "$caddy" portless-http "$portless_config" \
    "http://127.0.0.1/__1context/health" \
    "-H Host:wiki.1context.localhost"
fi

cat <<REPORT

Mode: high-port-https-probe
  URL: https://wiki.1context.localhost:$PROBE_HTTPS_PORT/your-context
  Expected permission/trust: none for Caddy process; browser trust is not installed
  Trust behavior: skip_install_trust prevents automatic root installation
  Purpose: live proof for local TLS mechanics without touching system trust
REPORT
validate_config "$caddy" high-port-https-probe "$https_probe_config"
if [[ "$RUN_LIVE" == "1" ]]; then
  run_probe "$caddy" high-port-https-probe "$https_probe_config" \
    "https://wiki.1context.localhost:$PROBE_HTTPS_PORT/__1context/health" \
    "--insecure --resolve wiki.1context.localhost:$PROBE_HTTPS_PORT:127.0.0.1"
fi

cat <<REPORT

Mode: local-https-direct-bind
  URL: https://wiki.1context.localhost/your-context
  Expected permission/trust: local CA trust plus privileged bind for port 443
  Trust behavior: this probe disables automatic trust installation
  Purpose: prove why Caddy should not be the root-owned process in product mode
REPORT
validate_config "$caddy" local-https "$https_config"
if [[ "$RUN_PRIVILEGED" == "1" ]]; then
  run_probe "$caddy" local-https "$https_config" \
    "https://wiki.1context.localhost/__1context/health" \
    "--insecure --resolve wiki.1context.localhost:443:127.0.0.1"
fi

if [[ "$SHOW_TRUST" == "1" ]]; then
  cat <<REPORT

Local Trust Manual Probe

The product path is the app's native setup flow, with `1context setup local-web
install` retained as the support/automation path. It prepares the Caddy local
CA, registers the bundled 127.0.0.1:443 ServiceManagement proxy, and records
uninstall metadata.
To test Caddy's own trust behavior separately, run a disposable trust probe
manually. This may prompt for an admin password and writes to macOS trust
settings:

  WORK_DIR="$WORK_DIR" \\
  HOME="$WORK_DIR/home" \\
  XDG_DATA_HOME="$WORK_DIR/xdg-data" \\
  XDG_CONFIG_HOME="$WORK_DIR/xdg-config" \\
  "$caddy" trust

Then start the HTTPS config and test Safari/Chrome:

  HOME="$WORK_DIR/home" XDG_DATA_HOME="$WORK_DIR/xdg-data" XDG_CONFIG_HOME="$WORK_DIR/xdg-config" \\
    "$caddy" run --config "$https_probe_config"

Cleanup must remove any trusted local CA that Caddy installed. Product setup
uses the 1Context installer path instead of allowing Caddy to silently mutate
system trust.
REPORT
fi

cat <<'REPORT'

Decision prompt:
  Product default is now the professional local HTTPS target, blocked behind
  explicit setup. The shipped architecture is a user-owned high-port Caddy TLS
  backend plus a bundled 127.0.0.1:443 ServiceManagement TCP proxy. High-port HTTP remains
  useful for deterministic harnesses, but should not be a silent product
  fallback when privileged bind or local trust setup is missing.
REPORT

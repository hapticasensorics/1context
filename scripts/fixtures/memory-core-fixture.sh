#!/usr/bin/env sh
set -eu

json_ok() {
  printf '%s\n' "$1"
}

case "${1:-}" in
  --version)
    printf '1context-memory-core-fixture 0.0.0\n'
    ;;
  status)
    json_ok '{"status":"ok","schema_version":1,"kind":"fixture","capabilities":["status","storage","wiki","memory"]}'
    ;;
  storage)
    case "${2:-}" in
      init)
        json_ok '{"status":"ok","schema_version":1,"command":"storage init","created":true}'
        ;;
      *)
        printf 'unsupported storage command: %s\n' "${2:-}" >&2
        exit 64
        ;;
    esac
    ;;
  wiki)
    case "${2:-}" in
      list)
        json_ok '{"status":"ok","schema_version":1,"wikis":[{"id":"default","path":"~/1Context/default"}]}'
        ;;
      ensure)
        json_ok '{"status":"ok","schema_version":1,"wiki":"default","ensured":true}'
        ;;
      render)
        json_ok '{"status":"ok","schema_version":1,"wiki":"default","rendered":["index.md"]}'
        ;;
      routes)
        json_ok '{"status":"ok","schema_version":1,"routes":[{"name":"daily","path":"~/1Context/default/daily"}]}'
        ;;
      *)
        printf 'unsupported wiki command: %s\n' "${2:-}" >&2
        exit 64
        ;;
    esac
    ;;
  memory)
    case "${2:-}" in
      tick)
        if [ "${3:-}" = "--wiki-only" ]; then
          json_ok '{"status":"ok","schema_version":1,"mode":"wiki-only","events":0}'
        else
          printf 'memory tick requires --wiki-only in public fixture\n' >&2
          exit 64
        fi
        ;;
      replay-dry-run)
        json_ok '{"status":"ok","schema_version":1,"dry_run":true,"changes":[]}'
        ;;
      cycles)
        case "${3:-}" in
          list)
            json_ok '{"status":"ok","schema_version":1,"cycles":[]}'
            ;;
          show)
            json_ok '{"status":"ok","schema_version":1,"cycle":{"id":"fixture"}}'
            ;;
          validate)
            json_ok '{"status":"ok","schema_version":1,"valid":true}'
            ;;
          *)
            printf 'unsupported memory cycles command: %s\n' "${3:-}" >&2
            exit 64
            ;;
        esac
        ;;
      *)
        printf 'unsupported memory command: %s\n' "${2:-}" >&2
        exit 64
        ;;
    esac
    ;;
  *)
    printf 'unsupported command: %s\n' "${1:-}" >&2
    exit 64
    ;;
esac

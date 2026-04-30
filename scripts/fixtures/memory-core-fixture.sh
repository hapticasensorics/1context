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
        if [ "${3:-}" = "--start" ] && [ -n "${4:-}" ] \
          && [ "${5:-}" = "--end" ] && [ -n "${6:-}" ] \
          && [ "${7:-}" = "--sources" ] && [ -n "${8:-}" ] \
          && [ "${9:-}" = "--replay-run-id" ] && [ -n "${10:-}" ] \
          && [ "${11:-}" = "--json" ]; then
          json_ok '{"status":"ok","schema_version":1,"dry_run":true,"changes":[]}'
        else
          printf 'memory replay-dry-run requires bounded replay parameters\n' >&2
          exit 64
        fi
        ;;
      cycles)
        case "${3:-}" in
          list)
            json_ok '{"status":"ok","schema_version":1,"cycles":[]}'
            ;;
          show)
            if [ -z "${4:-}" ] || [ "${5:-}" != "--json" ]; then
              printf 'memory cycles show requires cycle id\n' >&2
              exit 64
            fi
            json_ok '{"status":"ok","schema_version":1,"cycle":{"id":"fixture"}}'
            ;;
          validate)
            if [ -z "${4:-}" ] || [ "${5:-}" != "--json" ]; then
              printf 'memory cycles validate requires cycle id\n' >&2
              exit 64
            fi
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

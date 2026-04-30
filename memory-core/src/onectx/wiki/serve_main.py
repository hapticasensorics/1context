from __future__ import annotations

import argparse
from pathlib import Path

from ..config import load_system
from .server import DEFAULT_WIKI_HOST, DEFAULT_WIKI_PORT, serve_wiki


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m onectx.wiki.serve_main")
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--host", default=DEFAULT_WIKI_HOST)
    parser.add_argument("--port", type=int, default=DEFAULT_WIKI_PORT)
    parser.add_argument("--no-port-fallback", action="store_true")
    args = parser.parse_args(argv)

    serve_wiki(
        load_system(args.root),
        host=args.host,
        port=args.port,
        allow_port_fallback=not args.no_port_fallback,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

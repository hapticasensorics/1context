from __future__ import annotations

import errno
import hashlib
import json
import os
import subprocess
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlsplit

from ..config import MemorySystem
from .families import discover_families
from .librarian import answer as librarian_answer
from .librarian import chat_config, reset_chat, set_preferred_provider
from .routes import RouteTable, content_type, load_route_table, normalize_route, strip_owner_prefix
from .site import (
    CONTENT_INDEX_FILENAME,
    SITE_MANIFEST_FILENAME,
    WIKI_STATS_FILENAME,
    build_site_manifest,
    load_content_index,
    load_wiki_stats,
)
from .state import load_user_state, save_user_state, user_state_path


DEFAULT_WIKI_HOST = "127.0.0.1"
DEFAULT_WIKI_PORT = 17319
PENDING_TOP_LEVEL_ROUTES = {
    "/for-you": "For You",
    "/your-context": "Your Context",
    "/weekly-status": "Weekly Status",
    "/projects": "Projects",
    "/topics": "Topics",
}


def serve_wiki(
    system: MemorySystem,
    *,
    host: str = DEFAULT_WIKI_HOST,
    port: int = DEFAULT_WIKI_PORT,
    allow_port_fallback: bool = True,
) -> None:
    server = create_wiki_server(system, host=host, port=port, allow_port_fallback=allow_port_fallback)
    actual_host, actual_port = server.server_address[:2]
    try:
        print(f"1Context wiki serving at http://{actual_host}:{actual_port}/", flush=True)
        if port != 0 and actual_port != port:
            print(f"default port {port} was busy; using {actual_port} instead", flush=True)
        server.serve_forever()
    except KeyboardInterrupt:
        print("stopping 1Context wiki server")
    finally:
        server.server_close()


def wiki_url(path: str = "/", *, host: str = DEFAULT_WIKI_HOST, port: int = DEFAULT_WIKI_PORT) -> str:
    route = normalize_route(path)
    return f"http://{host}:{port}{route}"


def open_wiki_url(path: str = "/", *, host: str = DEFAULT_WIKI_HOST, port: int = DEFAULT_WIKI_PORT) -> str:
    url = wiki_url(path, host=host, port=port)
    subprocess.run(["open", url], check=True)
    return url


def create_wiki_server(
    system: MemorySystem,
    *,
    host: str = DEFAULT_WIKI_HOST,
    port: int = DEFAULT_WIKI_PORT,
    allow_port_fallback: bool = True,
    max_port_tries: int = 25,
) -> WikiHTTPServer:
    if port == 0:
        return WikiHTTPServer((host, port), WikiRequestHandler, system=system)

    last_error: OSError | None = None
    attempts = max(1, max_port_tries if allow_port_fallback else 1)
    for offset in range(attempts):
        candidate = port + offset
        try:
            return WikiHTTPServer((host, candidate), WikiRequestHandler, system=system)
        except OSError as exc:
            if exc.errno != errno.EADDRINUSE:
                raise
            last_error = exc
            if not allow_port_fallback:
                raise

    raise OSError(errno.EADDRINUSE, f"no available wiki port in range {port}-{port + attempts - 1}") from last_error


class WikiHTTPServer(ThreadingHTTPServer):
    allow_reuse_address = True

    def __init__(self, server_address: tuple[str, int], handler_class: type[BaseHTTPRequestHandler], *, system: MemorySystem):
        super().__init__(server_address, handler_class)
        self.system = system
        self.server_token = os.environ.get("ONECONTEXT_WIKI_SERVER_TOKEN") or ""


class WikiRequestHandler(BaseHTTPRequestHandler):
    server_version = "1ContextWiki/0.1"

    def do_HEAD(self) -> None:
        self.handle_request(send_body=False)

    def do_GET(self) -> None:
        self.handle_request(send_body=True)

    def do_POST(self) -> None:
        self.handle_post()

    def do_PATCH(self) -> None:
        self.handle_post()

    def log_message(self, format: str, *args: Any) -> None:
        return

    @property
    def system(self) -> MemorySystem:
        return self.server.system  # type: ignore[attr-defined]

    def handle_request(self, *, send_body: bool) -> None:
        if not self.request_host_allowed():
            self.send_json({"error": "forbidden", "message": "Request host is not allowed."}, status=HTTPStatus.FORBIDDEN, send_body=send_body)
            return
        route = normalize_route(self.path)
        table = load_route_table(self.system.root)

        if route == "/__health":
            payload: dict[str, Any] = {
                "status": "ok",
                "routes": len(table.routes),
                "manifests": len(table.manifests),
                "chat": chat_config(self.system),
            }
            challenge = self.headers.get("X-1Context-Wiki-Challenge") or ""
            if self.server.server_token and challenge:  # type: ignore[attr-defined]
                payload["server_token_proof"] = token_proof(self.server.server_token, challenge)  # type: ignore[attr-defined]
            self.send_json(payload, send_body=send_body)
            return

        if route == "/_routes":
            self.send_json(table.to_payload(), send_body=send_body)
            return

        if route in {f"/{SITE_MANIFEST_FILENAME}", "/api/wiki/site"}:
            self.send_json(build_site_manifest(self.system.root), send_body=send_body)
            return

        if route in {f"/{CONTENT_INDEX_FILENAME}", "/api/wiki/pages"}:
            self.send_json(load_content_index(self.system.root), send_body=send_body)
            return

        if route in {f"/{WIKI_STATS_FILENAME}", "/api/wiki/stats"}:
            self.send_json(load_wiki_stats(self.system.root), send_body=send_body)
            return

        if route == "/api/wiki/search":
            self.send_json(search_payload(self.system.root, self.path), send_body=send_body)
            return

        if route == "/api/wiki/state":
            state, exists = load_user_state(self.system)
            state["_storage"] = {
                "exists": exists,
                "path": format_state_path(user_state_path(self.system), self.system.root),
            }
            self.send_json(state, send_body=send_body)
            return

        if route == "/api/wiki/bookmarks":
            state, _ = load_user_state(self.system)
            self.send_json({"bookmarks": state.get("bookmarks", [])}, send_body=send_body)
            return

        if route == "/api/wiki/chat/config":
            self.send_json(chat_config(self.system), send_body=send_body)
            return

        asset = self.asset_for_route(route)
        if asset:
            self.send_bytes(asset["body"], content_type=asset["content_type"], send_body=send_body)
            return

        target = table.resolve(route)
        if target:
            self.send_file(target.path, target.content_type, send_body=send_body)
            return

        if route == "/":
            self.send_bytes(index_html(table), content_type="text/html; charset=utf-8", send_body=send_body)
            return

        pending = pending_route_label(route)
        if pending:
            self.send_bytes(
                pending_html(route, pending, self.system.root),
                content_type="text/html; charset=utf-8",
                send_body=send_body,
            )
            return

        self.send_bytes(
            not_found_html(route, table),
            status=HTTPStatus.NOT_FOUND,
            content_type="text/html; charset=utf-8",
            send_body=send_body,
        )

    def handle_post(self) -> None:
        if not self.request_host_allowed() or not self.unsafe_request_allowed():
            self.send_json({"error": "forbidden", "message": "Request origin is not allowed."}, status=HTTPStatus.FORBIDDEN, send_body=True)
            return
        route = normalize_route(self.path)
        try:
            payload = self.read_json_body()
        except ValueError as exc:
            self.send_json({"error": "bad_json", "message": str(exc)}, status=HTTPStatus.BAD_REQUEST, send_body=True)
            return

        if route == "/api/wiki/chat/provider":
            provider = str(payload.get("provider") or "auto").strip().lower()
            try:
                result = set_preferred_provider(self.system, provider)
            except ValueError as exc:
                self.send_json({"error": "bad_provider", "message": str(exc)}, status=HTTPStatus.BAD_REQUEST, send_body=True)
                return
            self.send_json(result, send_body=True)
            return

        if route == "/api/wiki/chat/reset":
            provider = payload.get("provider")
            result = reset_chat(self.system, str(provider).strip().lower() if provider else None)
            self.send_json(result, send_body=True)
            return

        if route == "/api/wiki/chat":
            status, result = librarian_answer(self.system, payload)
            self.send_json(result, status=HTTPStatus(status), send_body=True)
            return

        if route == "/api/wiki/state":
            state = save_user_state(self.system, payload)
            self.send_json(state, send_body=True)
            return

        if route == "/api/wiki/bookmarks":
            state = save_user_state(self.system, {"bookmarks": payload.get("bookmarks", [])})
            self.send_json({"bookmarks": state.get("bookmarks", [])}, send_body=True)
            return

        self.send_json({"error": "not_found", "message": f"No POST route exists for {route}."}, status=HTTPStatus.NOT_FOUND, send_body=True)

    def asset_for_route(self, route: str) -> dict[str, Any] | None:
        normalized = route if route.startswith("/assets/") else (strip_owner_prefix(route) or route)
        engine_root = self.system.root / "wiki-engine"
        if normalized == "/assets/theme.css":
            path = engine_root / "theme" / "css" / "theme.css"
            return {"body": path.read_bytes(), "content_type": content_type(path)} if path.exists() else None
        if normalized == "/assets/enhance.js":
            path = engine_root / "theme" / "js" / "enhance.js"
            return {"body": path.read_bytes(), "content_type": content_type(path)} if path.exists() else None
        if normalized == "/fish-logo.png":
            path = engine_root / "theme" / "assets" / "onecontext-icon-64.png"
            return {"body": path.read_bytes(), "content_type": content_type(path)} if path.exists() else None
        if normalized.startswith("/assets/"):
            asset_name = normalized.removeprefix("/assets/")
            if not asset_name or "/" in asset_name or asset_name in {".", ".."}:
                return None
            path = engine_root / "theme" / "assets" / asset_name
            return {"body": path.read_bytes(), "content_type": content_type(path)} if path.is_file() else None
        return None

    def read_json_body(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length") or "0")
        if length > 1_000_000:
            raise ValueError("request body is too large")
        raw = self.rfile.read(length) if length else b"{}"
        try:
            payload = json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError as exc:
            raise ValueError(str(exc)) from exc
        if not isinstance(payload, dict):
            raise ValueError("JSON body must be an object")
        return payload

    def send_json(self, payload: dict[str, Any], *, send_body: bool, status: HTTPStatus = HTTPStatus.OK) -> None:
        self.send_bytes(
            json.dumps(payload, indent=2, sort_keys=True).encode("utf-8") + b"\n",
            content_type="application/json; charset=utf-8",
            send_body=send_body,
            status=status,
        )

    def send_file(self, path: Path, declared_content_type: str, *, send_body: bool) -> None:
        try:
            path.resolve().relative_to(self.system.root.resolve())
        except ValueError:
            self.send_error(HTTPStatus.NOT_FOUND)
            return
        try:
            body = path.read_bytes()
        except OSError:
            self.send_error(HTTPStatus.NOT_FOUND)
            return
        self.send_bytes(body, content_type=declared_content_type, send_body=send_body)

    def send_bytes(
        self,
        body: bytes | str,
        *,
        content_type: str,
        send_body: bool,
        status: HTTPStatus = HTTPStatus.OK,
    ) -> None:
        if isinstance(body, str):
            body = body.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def request_host_allowed(self) -> bool:
        host = host_name(self.headers.get("Host") or "").lower()
        if not host:
            return True
        return host in {"127.0.0.1", "localhost", "::1"}

    def unsafe_request_allowed(self) -> bool:
        host = self.headers.get("Host") or ""
        for header in ("Origin", "Referer"):
            value = self.headers.get(header)
            if not value:
                continue
            parsed = urlsplit(value)
            if parsed.scheme not in {"http", "https"}:
                return False
            if parsed.netloc.lower() != host.lower():
                return False
        return True


def host_name(value: str) -> str:
    value = value.strip()
    if not value:
        return ""
    if value.startswith("["):
        end = value.find("]")
        return value[1:end] if end > 0 else value
    return value.split(":", 1)[0]


def token_proof(token: str, challenge: str) -> str:
    return hashlib.sha256(f"{token}:{challenge}".encode("utf-8")).hexdigest()


def index_html(table: RouteTable) -> bytes:
    families = discover_families(table.root)
    route_items = []
    for family in families:
        target = table.resolve(family.route)
        state = "ready" if target else "not rendered"
        href = family.route if target else "/_routes"
        route_items.append(f'<li><a href="{escape_html(href)}">{escape_html(family.label)}</a> <span>{state}</span></li>')
    if not route_items:
        route_items.append("<li>No wiki families discovered.</li>")
    body = "\n".join(route_items)
    # Redirect "/" to /your-context — the canonical entry point for visitors.
    # The dev route-table is still available at /_routes for development.
    return b"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>1Context</title>
  <meta http-equiv="refresh" content="0; url=/your-context">
  <link rel="canonical" href="/your-context">
</head>
<body>
  <p>Redirecting to <a href="/your-context">/your-context</a>.</p>
</body>
</html>
"""


def not_found_html(route: str, table: RouteTable) -> bytes:
    route_list = "\n".join(
        f'<li><a href="{escape_html(item.route)}">{escape_html(item.route)}</a></li>'
        for item in sorted(table.routes.values(), key=lambda value: value.route)[:40]
    )
    body = f"""<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Not Found - 1Context Wiki</title></head>
<body>
  <h1>Not Found</h1>
  <p>No rendered wiki route exists for <code>{escape_html(route)}</code>.</p>
  <h2>Available Routes</h2>
  <ul>{route_list}</ul>
</body>
</html>
"""
    return body.encode("utf-8")


def pending_route_label(route: str) -> str:
    normalized = strip_owner_prefix(route) or route
    if route in {"/paul-demo2", "/paul-demo2/"}:
        return "1Context Wiki"
    return PENDING_TOP_LEVEL_ROUTES.get(normalized, "")


def format_state_path(path: Path, root: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)


def pending_html(route: str, label: str, root: Path) -> bytes:
    seed = pending_seed_html(root, route)
    if seed:
        description = "<p>Preparing your wiki. 1Context is setting up the local wiki engine; this template page is available while the rendered page warms up.</p>"
        content = seed
    else:
        description = "<p>Preparing your wiki. This local route is reserved, but no rendered family is available yet.</p>"
        content = f"<p><code>{escape_html(route)}</code></p>"
    back_link = "" if route == "/for-you" else '<p><a href="/for-you">Back to For You</a></p>'
    body = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{escape_html(label)} - 1Context Wiki</title>
  <style>body{{font:16px/1.5 system-ui,sans-serif;max-width:760px;margin:48px auto;padding:0 20px}} a{{color:#174ea6}} code{{background:#f3f4f6;padding:2px 4px;border-radius:4px}} .notice{{color:#667085}}</style>
</head>
<body>
  <h1>{escape_html(label)}</h1>
  <div class="notice">{description}</div>
  {content}
  {back_link}
</body>
</html>
"""
    return body.encode("utf-8")


def pending_seed_html(root: Path, route: str) -> str:
    return ""


def markdown_fragment_html(markdown: str) -> str:
    body = strip_frontmatter(markdown).strip()
    if not body:
        return ""
    lines = body.splitlines()
    html: list[str] = []
    paragraph: list[str] = []
    in_list = False

    def flush_paragraph() -> None:
        nonlocal paragraph
        if paragraph:
            html.append(f"<p>{escape_html(' '.join(paragraph))}</p>")
            paragraph = []

    def close_list() -> None:
        nonlocal in_list
        if in_list:
            html.append("</ul>")
            in_list = False

    for raw_line in lines:
        line = raw_line.strip()
        if not line or line.startswith("<!--"):
            flush_paragraph()
            close_list()
            continue
        if line.startswith("# "):
            flush_paragraph()
            close_list()
            html.append(f"<h2>{escape_html(line.removeprefix('# ').strip())}</h2>")
        elif line.startswith("## "):
            flush_paragraph()
            close_list()
            html.append(f"<h3>{escape_html(line.removeprefix('## ').strip())}</h3>")
        elif line.startswith("- "):
            flush_paragraph()
            if not in_list:
                html.append("<ul>")
                in_list = True
            html.append(f"<li>{escape_html(line.removeprefix('- ').strip())}</li>")
        else:
            paragraph.append(line)

    flush_paragraph()
    close_list()
    return "\n".join(html)


def strip_frontmatter(markdown: str) -> str:
    if not markdown.startswith("---\n"):
        return markdown
    end = markdown.find("\n---\n", 4)
    if end == -1:
        return markdown
    return markdown[end + 5 :]


def search_payload(root: Path, raw_path: str) -> dict[str, Any]:
    query = parse_qs(urlsplit(raw_path).query).get("q", [""])[0].strip()
    index = load_content_index(root)
    if not query:
        return {"query": query, "matches": []}
    terms = [term.lower() for term in query.split() if term.strip()]
    matches_by_key: dict[str, dict[str, Any]] = {}
    for page in index.get("pages", []):
        if not isinstance(page, dict):
            continue
        haystack = " ".join(
            str(page.get(key) or "")
            for key in ("title", "slug", "route", "summary", "excerpt", "markdown")
        ).lower()
        if all(term in haystack for term in terms):
            result = search_result(page, terms)
            key = str(page.get("slug") or result["route"])
            existing = matches_by_key.get(key)
            if not existing or result["_score"] > existing["_score"]:
                matches_by_key[key] = result
    matches = sorted(
        matches_by_key.values(),
        key=lambda item: (-int(item.get("_score", 0)), str(item.get("title", "")).lower(), str(item.get("route", ""))),
    )[:20]
    for item in matches:
        item.pop("_score", None)
    return {"query": query, "matches": matches, "pages": matches}


def search_result(page: dict[str, Any], terms: list[str]) -> dict[str, Any]:
    route = canonical_result_route(str(page.get("route") or ""))
    title = str(page.get("title") or route)
    summary = str(page.get("summary") or "")
    excerpt = str(page.get("excerpt") or summary)
    score = score_page(page, terms)
    return {
        "_score": score,
        "title": title,
        "matched_title": highlighted(title, terms),
        "route": route,
        "url": route,
        "family_id": str(page.get("family_id") or ""),
        "family_label": str(page.get("family_label") or ""),
        "summary": summary,
        "description": summary or excerpt,
        "excerpt": excerpt,
        "source": str(page.get("source") or ""),
        "path": str(page.get("path") or ""),
    }


def score_page(page: dict[str, Any], terms: list[str]) -> int:
    title = str(page.get("title") or "").lower()
    slug = str(page.get("slug") or "").lower()
    route = str(page.get("route") or "").lower()
    summary = str(page.get("summary") or "").lower()
    excerpt = str(page.get("excerpt") or "").lower()
    markdown = str(page.get("markdown") or "").lower()
    score = 0
    for term in terms:
        if title == term or slug == term:
            score += 120
        elif title.startswith(term) or slug.startswith(term):
            score += 90
        elif term in title or term in slug or term in route:
            score += 70
        elif term in summary:
            score += 40
        elif term in excerpt:
            score += 25
        elif term in markdown:
            score += 10
    if page.get("source") == "source":
        score += 8
    elif page.get("source") == "generated_markdown":
        score -= 8
    return score


def canonical_result_route(route: str) -> str:
    if route.endswith(".md"):
        return route[:-3]
    if route.endswith(".html"):
        return route[:-5]
    return route or "/"


def highlighted(text: str, terms: list[str]) -> str:
    result = escape_html(text)
    for term in sorted(terms, key=len, reverse=True):
        if not term:
            continue
        result = result.replace(escape_html(term), f"<strong>{escape_html(term)}</strong>")
        result = result.replace(escape_html(term.title()), f"<strong>{escape_html(term.title())}</strong>")
    return result


def escape_html(value: str) -> str:
    return (
        value.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )

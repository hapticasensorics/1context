from __future__ import annotations

import json
import socket
import threading
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from onectx.config import load_system
from onectx.wiki.server import create_wiki_server
from onectx.wiki.routes import load_route_table


def test_wiki_server_falls_back_when_requested_port_is_busy() -> None:
    system = load_system(Path.cwd())
    occupied = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    occupied.bind(("127.0.0.1", 0))
    occupied.listen(1)
    busy_port = occupied.getsockname()[1]

    try:
        server = create_wiki_server(system, host="127.0.0.1", port=busy_port, max_port_tries=4)
        try:
            assert server.server_address[1] != busy_port
            assert busy_port < server.server_address[1] <= busy_port + 3
        finally:
            server.server_close()
    finally:
        occupied.close()


def test_wiki_server_health_responds_over_http() -> None:
    system = load_system(Path.cwd())
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        with urlopen(f"http://{host}:{port}/__health", timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
        assert payload["status"] == "ok"
        assert "routes" in payload
        assert "manifests" in payload
        assert payload["routes"] is None
        assert payload["manifests"] is None

        with urlopen(f"http://{host}:{port}/__health?details=1", timeout=5) as response:
            detailed = json.loads(response.read().decode("utf-8"))
        assert detailed["status"] == "ok"
        assert isinstance(detailed["routes"], int)
        assert isinstance(detailed["manifests"], int)
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def test_wiki_server_health_reports_launch_token(monkeypatch) -> None:
    monkeypatch.setenv("ONECONTEXT_WIKI_SERVER_TOKEN", "test-token")
    system = load_system(Path.cwd())
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        with urlopen(f"http://{host}:{port}/__health", timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
        assert payload["status"] == "ok"
        assert "server_token" not in payload

        request = Request(
            f"http://{host}:{port}/__health",
            headers={"X-1Context-Wiki-Challenge": "challenge"},
        )
        with urlopen(request, timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
        assert payload["status"] == "ok"
        assert payload["server_token_proof"]
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def test_wiki_server_health_does_not_load_route_table(monkeypatch) -> None:
    def fail_route_load(root: Path):
        raise AssertionError(f"health should not load routes for {root}")

    monkeypatch.setattr("onectx.wiki.server.load_route_table", fail_route_load)
    system = load_system(Path.cwd())
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        with urlopen(f"http://{host}:{port}/__health", timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
        assert payload["status"] == "ok"
        assert payload["routes"] is None
        assert payload["manifests"] is None
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def test_wiki_server_stats_endpoint_responds_over_http() -> None:
    system = load_system(Path.cwd())
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        with urlopen(f"http://{host}:{port}/api/wiki/stats", timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
        assert payload["schema_version"] == "wiki.stats.v1"
        assert payload["totals"]["families"] >= 1
        assert "families" in payload
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def test_wiki_server_rejects_cross_origin_post() -> None:
    system = load_system(Path.cwd())
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        request = Request(
            f"http://{host}:{port}/api/wiki/bookmarks",
            data=b'{"bookmarks":[]}',
            headers={"Content-Type": "application/json", "Origin": "https://example.invalid"},
            method="POST",
        )
        try:
            urlopen(request, timeout=5)
        except HTTPError as exc:
            assert exc.code == 403
        else:
            raise AssertionError("cross-origin POST should be rejected")
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def test_wiki_server_rejects_bad_host_gets() -> None:
    system = load_system(Path.cwd())
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        request = Request(f"http://{host}:{port}/__health", headers={"Host": "attacker.invalid"})
        try:
            urlopen(request, timeout=5)
        except HTTPError as exc:
            assert exc.code == 403
        else:
            raise AssertionError("bad host GET should be rejected")
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def test_wiki_server_accepts_same_origin_state_post() -> None:
    system = load_system(Path.cwd())
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    origin = f"http://{host}:{port}"
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        request = Request(
            f"{origin}/api/wiki/bookmarks",
            data=b'{"bookmarks":[{"url":"/for-you","title":"For You"}]}',
            headers={"Content-Type": "application/json", "Origin": origin},
            method="POST",
        )
        with urlopen(request, timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
        assert payload["bookmarks"][0]["url"] == "/for-you"

        with urlopen(f"{origin}/api/wiki/bookmarks", timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
        assert payload["bookmarks"][0]["url"] == "/for-you"
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def test_route_table_ignores_manifest_paths_outside_root(tmp_path: Path) -> None:
    manifest_dir = tmp_path / "wiki" / "menu" / "private" / "generated"
    manifest_dir.mkdir(parents=True)
    outside = tmp_path.parent / "outside-secret.md"
    outside.write_text("secret", encoding="utf-8")
    (manifest_dir / "render-manifest.json").write_text(
        json.dumps(
            {
                "family": {"id": "private"},
                "routes": [
                    {
                        "route": "/secret",
                        "output_path": str(outside),
                    }
                ],
            }
        ),
        encoding="utf-8",
    )

    table = load_route_table(tmp_path)

    assert table.resolve("/secret") is None


def test_route_table_ignores_manifest_paths_outside_generated_dir(tmp_path: Path) -> None:
    manifest_dir = tmp_path / "wiki" / "menu" / "private" / "generated"
    manifest_dir.mkdir(parents=True)
    sibling_secret = tmp_path / "wiki" / "menu" / "private" / "secret.html"
    sibling_secret.write_text("<p>secret</p>", encoding="utf-8")
    (manifest_dir / "safe.html").write_text("<p>safe</p>", encoding="utf-8")
    (manifest_dir / "render-manifest.json").write_text(
        json.dumps(
            {
                "family": {"id": "private"},
                "routes": [
                    {"route": "/secret", "output_path": "../secret.html"},
                    {"route": "/safe", "output_path": "safe.html"},
                ],
            }
        ),
        encoding="utf-8",
    )

    table = load_route_table(tmp_path)

    assert table.resolve("/secret") is None
    assert table.resolve("/safe") is not None


def test_route_table_serves_only_public_html_outputs(tmp_path: Path) -> None:
    manifest_dir = tmp_path / "wiki" / "menu" / "private" / "generated"
    manifest_dir.mkdir(parents=True)
    for name in ["page.html", "page.private.html", "page.internal.html", "page.talk.html", "page.md"]:
        (manifest_dir / name).write_text(name, encoding="utf-8")
    (manifest_dir / "render-manifest.json").write_text(
        json.dumps(
            {
                "family": {"id": "private"},
                "outputs": [
                    {"path": "page.html"},
                    {"path": "page.private.html"},
                    {"path": "page.internal.html"},
                    {"path": "page.talk.html"},
                    {"path": "page.md"},
                ],
                "routes": [
                    {"route": "/private", "output_path": "page.private.html"},
                    {"route": "/internal", "output_path": "page.internal.html"},
                    {"route": "/talk", "output_path": "page.talk.html"},
                    {"route": "/raw", "output_path": "page.md"},
                ],
            }
        ),
        encoding="utf-8",
    )

    table = load_route_table(tmp_path)

    assert table.resolve("/page") is not None
    assert table.resolve("/private") is None
    assert table.resolve("/internal") is None
    assert table.resolve("/talk") is None
    assert table.resolve("/raw") is None
    assert table.resolve("/page.md") is None


def test_pending_route_does_not_render_private_source(tmp_path: Path) -> None:
    family = tmp_path / "wiki" / "menu" / "10-for-you" / "10-for-you"
    source = family / "source"
    source.mkdir(parents=True)
    (tmp_path / "wiki" / "menu" / "10-for-you" / "group.toml").write_text('id = "for-you"\n', encoding="utf-8")
    (family / "family.toml").write_text(
        'id = "for-you"\nroute = "/for-you"\n[source]\ndir = "source"\nprimary = "source/private.md"\n',
        encoding="utf-8",
    )
    (source / "private.md").write_text("# Private\n\nPRIVATE_ONLY_PHRASE\n", encoding="utf-8")
    system = type("System", (), {"root": tmp_path, "runtime_dir": tmp_path / "runtime"})()
    server = create_wiki_server(system, host="127.0.0.1", port=0)
    host, port = server.server_address[:2]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        with urlopen(f"http://{host}:{port}/for-you", timeout=5) as response:
            body = response.read().decode("utf-8")
        assert "PRIVATE_ONLY_PHRASE" not in body
        assert "Preparing your wiki" in body
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()

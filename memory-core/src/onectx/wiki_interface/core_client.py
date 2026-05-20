from __future__ import annotations

import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class WikiCoreError(RuntimeError):
    """Raised when the portable wiki core cannot be invoked cleanly."""

    def __init__(self, message: str, *, payload: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.payload = payload

    @property
    def operation(self) -> str | None:
        if not isinstance(self.payload, dict):
            return None
        value = self.payload.get("operation")
        return value if isinstance(value, str) else None

    @property
    def error_code(self) -> str | None:
        error = self._error_payload()
        value = error.get("code") if error is not None else None
        return value if isinstance(value, str) else None

    @property
    def error_message(self) -> str | None:
        error = self._error_payload()
        value = error.get("message") if error is not None else None
        return value if isinstance(value, str) else None

    @property
    def repair_hints(self) -> list[str]:
        if not isinstance(self.payload, dict):
            return []
        value = self.payload.get("repair_hints")
        if not isinstance(value, list):
            return []
        return [hint for hint in value if isinstance(hint, str)]

    def _error_payload(self) -> dict[str, Any] | None:
        if not isinstance(self.payload, dict):
            return None
        error = self.payload.get("error")
        return error if isinstance(error, dict) else None


@dataclass(frozen=True)
class WikiCoreClient:
    """Thin Python client for the portable Rust wiki core.

    The memory system uses this client when it needs wiki semantics. It should
    not duplicate page placement, template fallback, talk delivery, inbox, or
    notification rules in Python.
    """

    runtime_home: Path
    binary: Path | str | None = None
    timeout_seconds: float = 120.0

    @property
    def onecontext_root(self) -> Path:
        root = Path(self.runtime_home)
        if root.name == "1Context" and (root / "user-wiki").exists():
            return root
        return root / "1Context"

    def call(self, *args: str) -> dict[str, Any]:
        binary = resolve_wiki_core_binary(self.binary)
        command = [str(binary), "--root", str(self.onecontext_root), *map(str, args)]
        result = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=self.timeout_seconds,
        )
        if result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip() or f"exit {result.returncode}"
            try:
                payload = json.loads(detail)
            except json.JSONDecodeError:
                payload = None
            if isinstance(payload, dict):
                error = payload.get("error")
                if isinstance(error, dict):
                    code = error.get("code")
                    message = error.get("message")
                    if code and message:
                        detail = f"{code}: {message}"
                    elif message:
                        detail = str(message)
                raise WikiCoreError(f"onecontext-wiki failed: {detail}", payload=payload)
            raise WikiCoreError(f"onecontext-wiki failed: {detail}")
        try:
            payload = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise WikiCoreError(f"onecontext-wiki returned invalid JSON: {result.stdout[:500]!r}") from exc
        if not isinstance(payload, dict):
            raise WikiCoreError("onecontext-wiki returned a non-object JSON payload")
        return payload

    def ensure(self) -> dict[str, Any]:
        return self.call("ensure")

    def status(self) -> dict[str, Any]:
        return self.call("status")

    def validate(self) -> dict[str, Any]:
        return self.call("validate")

    def list_pages(self) -> dict[str, Any]:
        return self.call("list")

    def page_status(self, page: str) -> dict[str, Any]:
        return self.call("page-status", page)

    def page_open(self, page: str) -> dict[str, Any]:
        return self.call("page-open", page)

    def page_create(
        self,
        page: str,
        *,
        title: str | None = None,
        slug: str | None = None,
        route: str | None = None,
        family_group: str | None = None,
        family_group_title: str | None = None,
        family_id: str | None = None,
        family_title: str | None = None,
        page_type: str | None = None,
        template: str | None = None,
        talk_conventions_template: str | None = None,
        talk_curator_template: str | None = None,
        summary: str | None = None,
        nav_order: int | None = None,
        nav_section: str | None = None,
    ) -> dict[str, Any]:
        args = ["page-create", page]
        for flag, value in [
            ("--title", title),
            ("--slug", slug),
            ("--route", route),
            ("--family-group", family_group),
            ("--family-group-title", family_group_title),
            ("--family-id", family_id),
            ("--family-title", family_title),
            ("--type", page_type),
            ("--template", template),
            ("--talk-conventions-template", talk_conventions_template),
            ("--talk-curator-template", talk_curator_template),
            ("--summary", summary),
            ("--nav-section", nav_section),
        ]:
            if value is not None:
                args.extend([flag, value])
        if nav_order is not None:
            args.extend(["--nav-order", str(nav_order)])
        return self.call(*args)

    def page_create_all(self) -> dict[str, Any]:
        return self.call("page-create-all")

    def page_write_body(
        self,
        page: str,
        *,
        body_markdown: str | None = None,
        body_file: Path | str | None = None,
        expected_source_sha256: str | None = None,
    ) -> dict[str, Any]:
        args = ["page-write-body", page]
        _extend_text_or_file_arg(
            args,
            operation="page-write-body",
            inline_flag="--body",
            inline_value=body_markdown,
            file_flag="--body-file",
            file_value=body_file,
        )
        if expected_source_sha256 is not None:
            args.extend(["--expected-source-sha256", expected_source_sha256])
        return self.call(*args)

    def page_patch_body(
        self,
        page: str,
        *,
        find: str | None = None,
        replace: str | None = None,
        find_file: Path | str | None = None,
        replace_file: Path | str | None = None,
        expected_source_sha256: str | None = None,
    ) -> dict[str, Any]:
        args = ["page-patch-body", page]
        _extend_text_or_file_arg(
            args,
            operation="page-patch-body",
            inline_flag="--find",
            inline_value=find,
            file_flag="--find-file",
            file_value=find_file,
        )
        _extend_text_or_file_arg(
            args,
            operation="page-patch-body",
            inline_flag="--replace",
            inline_value=replace,
            file_flag="--replace-file",
            file_value=replace_file,
        )
        if expected_source_sha256 is not None:
            args.extend(["--expected-source-sha256", expected_source_sha256])
        return self.call(*args)

    def page_delete(self, page: str, *, mode: str = "tombstone") -> dict[str, Any]:
        return self.call("page-delete", page, "--mode", mode)

    def page_restore(self, page: str) -> dict[str, Any]:
        return self.call("page-restore", page)

    def publish_status(self) -> dict[str, Any]:
        return self.call("publish-status")

    def publish(
        self,
        *,
        wiki_engine: Path | str | None = None,
        trigger: str = "agent",
        force: bool = False,
        node: str | None = None,
    ) -> dict[str, Any]:
        args = ["publish", "--trigger", trigger]
        if wiki_engine is not None:
            args.extend(["--wiki-engine", str(wiki_engine)])
        if node is not None:
            args.extend(["--node", node])
        if force:
            args.append("--force")
        return self.call(*args)

    def agent_register(
        self,
        *,
        thread_id: str,
        roles: list[str] | tuple[str, ...] = (),
        capabilities: list[str] | tuple[str, ...] = (),
        ttl_seconds: int = 1800,
    ) -> dict[str, Any]:
        args = ["agent-register", "--thread-id", thread_id, "--ttl-seconds", str(ttl_seconds)]
        for role in roles:
            args.extend(["--role", role])
        for capability in capabilities:
            args.extend(["--capability", capability])
        return self.call(*args)

    def agent_identify(
        self,
        *,
        thread_id: str,
        roles: list[str] | tuple[str, ...] = (),
        capabilities: list[str] | tuple[str, ...] = (),
        ttl_seconds: int = 1800,
    ) -> dict[str, Any]:
        args = ["agent-identify", "--thread-id", thread_id, "--ttl-seconds", str(ttl_seconds)]
        for role in roles:
            args.extend(["--role", role])
        for capability in capabilities:
            args.extend(["--capability", capability])
        return self.call(*args)

    def agent_heartbeat(self, agent_id: str, *, ttl_seconds: int = 1800) -> dict[str, Any]:
        return self.call("agent-heartbeat", agent_id, "--ttl-seconds", str(ttl_seconds))

    def agent_retire(self, agent_id: str, *, reason: str = "completed") -> dict[str, Any]:
        return self.call("agent-retire", agent_id, "--reason", reason)

    def agent_whoami(
        self,
        *,
        thread_id: str | None = None,
        agent_id: str | None = None,
    ) -> dict[str, Any]:
        args = ["whoami"]
        if thread_id is not None:
            args.extend(["--thread-id", thread_id])
        if agent_id is not None:
            args.extend(["--agent-id", agent_id])
        return self.call(*args)

    def agent_list(
        self,
        *,
        include_stale: bool = False,
        include_retired: bool = False,
    ) -> dict[str, Any]:
        args = ["agent-list"]
        if include_stale:
            args.append("--include-stale")
        if include_retired:
            args.append("--include-retired")
        return self.call(*args)

    def agent_status(self, agent_id: str) -> dict[str, Any]:
        return self.call("agent-status", agent_id)

    def talk_append(
        self,
        *,
        page: str,
        subject: str,
        from_address: str,
        to: list[str] | tuple[str, ...],
        body_markdown: str | None = None,
        body_file: Path | str | None = None,
        kind: str = "proposal",
        cc: list[str] | tuple[str, ...] = (),
        to_roles: list[str] | tuple[str, ...] = (),
        cc_roles: list[str] | tuple[str, ...] = (),
        attachments: list[Path | str] | tuple[Path | str, ...] = (),
        allow_tombstoned: bool = False,
        thread_id: str | None = None,
        reply_to: str | None = None,
    ) -> dict[str, Any]:
        args = [
            "talk-append",
            "--page",
            page,
            "--kind",
            kind,
            "--subject",
            subject,
            "--from",
            from_address,
        ]
        if thread_id is not None:
            args.extend(["--thread-id", thread_id])
        if reply_to is not None:
            args.extend(["--reply-to", reply_to])
        for recipient in to:
            args.extend(["--to", recipient])
        for recipient in cc:
            args.extend(["--cc", recipient])
        for role in to_roles:
            args.extend(["--to-role", role])
        for role in cc_roles:
            args.extend(["--cc-role", role])
        for attachment in attachments:
            args.extend(["--attachment", str(attachment)])
        _extend_text_or_file_arg(
            args,
            operation="talk-append",
            inline_flag="--body",
            inline_value=body_markdown,
            file_flag="--body-file",
            file_value=body_file,
        )
        if allow_tombstoned:
            args.append("--allow-tombstoned")
        return self.call(*args)

    def mail_inbox(
        self,
        recipient: str,
        *,
        include_archived: bool = False,
        include_snoozed: bool = False,
    ) -> dict[str, Any]:
        args = ["mail-inbox", recipient]
        if include_archived:
            args.append("--include-archived")
        if include_snoozed:
            args.append("--include-snoozed")
        return self.call(*args)

    def mail_read(
        self,
        *,
        message_id: str | None = None,
        thread_id: str | None = None,
    ) -> dict[str, Any]:
        args = ["mail-read"]
        if message_id is not None:
            args.extend(["--message-id", message_id])
        if thread_id is not None:
            args.extend(["--thread-id", thread_id])
        return self.call(*args)

    def mail_subscribe(
        self,
        *,
        agent_id: str,
        address: str,
        relation: str = "subscriber",
        kinds: list[str] | tuple[str, ...] = (),
        ttl_seconds: int = 1800,
    ) -> dict[str, Any]:
        args = [
            "mail-subscribe",
            "--agent-id",
            agent_id,
            "--address",
            address,
            "--relation",
            relation,
            "--ttl-seconds",
            str(ttl_seconds),
        ]
        for kind in kinds:
            args.extend(["--kind", kind])
        return self.call(*args)

    def mail_unsubscribe(
        self,
        *,
        agent_id: str,
        address: str,
        relation: str | None = None,
        kinds: list[str] | tuple[str, ...] = (),
    ) -> dict[str, Any]:
        args = ["mail-unsubscribe", "--agent-id", agent_id, "--address", address]
        if relation is not None:
            args.extend(["--relation", relation])
        for kind in kinds:
            args.extend(["--kind", kind])
        return self.call(*args)

    def mail_subscriptions(
        self,
        *,
        agent_id: str | None = None,
        address: str | None = None,
    ) -> dict[str, Any]:
        args = ["mail-subscriptions"]
        if agent_id is not None:
            args.extend(["--agent-id", agent_id])
        if address is not None:
            args.extend(["--address", address])
        return self.call(*args)

    def page_watch(
        self,
        page: str,
        *,
        agent_id: str,
        list_address: str | None = None,
        kinds: list[str] | tuple[str, ...] = (),
        ttl_seconds: int = 1800,
    ) -> dict[str, Any]:
        args = ["page-watch", page, "--agent-id", agent_id, "--ttl-seconds", str(ttl_seconds)]
        if list_address is not None:
            args.extend(["--list", list_address])
        for kind in kinds:
            args.extend(["--kind", kind])
        return self.call(*args)

    def page_unwatch(
        self,
        page: str,
        *,
        agent_id: str,
        list_address: str | None = None,
        kinds: list[str] | tuple[str, ...] = (),
    ) -> dict[str, Any]:
        args = ["page-unwatch", page, "--agent-id", agent_id]
        if list_address is not None:
            args.extend(["--list", list_address])
        for kind in kinds:
            args.extend(["--kind", kind])
        return self.call(*args)

    def page_assign_role(
        self,
        page: str,
        *,
        agent_id: str,
        role: str,
        kinds: list[str] | tuple[str, ...] = (),
        ttl_seconds: int = 1800,
    ) -> dict[str, Any]:
        args = [
            "page-assign-role",
            page,
            "--agent-id",
            agent_id,
            "--role",
            role,
            "--ttl-seconds",
            str(ttl_seconds),
        ]
        for kind in kinds:
            args.extend(["--kind", kind])
        return self.call(*args)

    def list_create(
        self,
        *,
        address: str,
        title: str | None = None,
        description: str | None = None,
        page: str | None = None,
        owner: str | None = None,
    ) -> dict[str, Any]:
        args = ["list-create", "--address", address]
        for flag, value in [
            ("--title", title),
            ("--description", description),
            ("--page", page),
            ("--owner", owner),
        ]:
            if value is not None:
                args.extend([flag, value])
        return self.call(*args)

    def lists(
        self,
        *,
        page: str | None = None,
        address: str | None = None,
    ) -> dict[str, Any]:
        args = ["lists"]
        if page is not None:
            args.extend(["--page", page])
        if address is not None:
            args.extend(["--address", address])
        return self.call(*args)

    def list_status(
        self,
        address: str,
        *,
        include_archived: bool = False,
        include_snoozed: bool = False,
    ) -> dict[str, Any]:
        args = ["list-status", address]
        if include_archived:
            args.append("--include-archived")
        if include_snoozed:
            args.append("--include-snoozed")
        return self.call(*args)

    def list_members(self, address: str) -> dict[str, Any]:
        return self.call("list-members", address)

    def agent_inbox(
        self,
        agent_id: str,
        *,
        include_archived: bool = False,
        include_snoozed: bool = False,
    ) -> dict[str, Any]:
        args = ["agent-inbox", agent_id]
        if include_archived:
            args.append("--include-archived")
        if include_snoozed:
            args.append("--include-snoozed")
        return self.call(*args)

    def agent_claim(self, agent_id: str, message_id: str) -> dict[str, Any]:
        return self.call("agent-claim", agent_id, message_id)

    def mail_mark(
        self,
        message_id: str,
        *,
        recipient: str,
        state: str,
        until: str | None = None,
    ) -> dict[str, Any]:
        args = ["mail-mark", message_id, "--recipient", recipient, "--state", state]
        if until is not None:
            args.extend(["--until", until])
        return self.call(*args)

    def mail_mark_all(
        self,
        message_id: str,
        *,
        state: str,
        until: str | None = None,
    ) -> dict[str, Any]:
        args = ["mail-mark-all", message_id, "--state", state]
        if until is not None:
            args.extend(["--until", until])
        return self.call(*args)

    def mail_claim(self, message_id: str, *, recipient: str, agent_id: str) -> dict[str, Any]:
        return self.call("mail-claim", message_id, "--recipient", recipient, "--agent-id", agent_id)

    def notify_poll(self, agent_id: str) -> dict[str, Any]:
        return self.call("notify-poll", agent_id)

    def notify_ack(self, notification_id: str, *, agent_id: str, state: str = "delivered") -> dict[str, Any]:
        return self.call("notify-ack", notification_id, "--agent-id", agent_id, "--state", state)


def resolve_wiki_core_binary(explicit: Path | str | None = None) -> Path:
    if explicit is not None:
        return _existing_binary(Path(explicit))

    env_value = os.environ.get("ONECONTEXT_WIKI_CORE_BIN")
    if env_value:
        return _existing_binary(Path(env_value))

    path_value = shutil.which("onecontext-wiki")
    if path_value:
        return _existing_binary(Path(path_value))

    repo_root = _find_repo_root()
    debug_candidate = repo_root / "target/debug/onecontext-wiki"
    if (repo_root / "Cargo.toml").is_file():
        if _dev_binary_is_stale(repo_root, debug_candidate):
            _build_dev_binary(repo_root)
        if debug_candidate.is_file():
            return debug_candidate

    release_candidate = repo_root / "target/release/onecontext-wiki"
    if release_candidate.is_file():
        return release_candidate

    raise WikiCoreError(
        "could not find onecontext-wiki; build it with "
        "`cargo build --package onecontext-wiki-daemon` or set ONECONTEXT_WIKI_CORE_BIN"
    )


def wiki_list(runtime_home: Path, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).list_pages()


def wiki_ensure(runtime_home: Path, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).ensure()


def wiki_status(runtime_home: Path, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).status()


def wiki_validate(runtime_home: Path, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).validate()


def wiki_page_status(runtime_home: Path, page: str, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_status(page)


def wiki_page_open(runtime_home: Path, page: str, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_open(page)


def wiki_page_create(
    runtime_home: Path,
    page: str,
    *,
    binary: Path | str | None = None,
    **kwargs: Any,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_create(page, **kwargs)


def wiki_page_create_all(runtime_home: Path, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_create_all()


def wiki_page_write_body(
    runtime_home: Path,
    page: str,
    *,
    body_markdown: str | None = None,
    body_file: Path | str | None = None,
    expected_source_sha256: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_write_body(
        page,
        body_markdown=body_markdown,
        body_file=body_file,
        expected_source_sha256=expected_source_sha256,
    )


def wiki_page_patch_body(
    runtime_home: Path,
    page: str,
    *,
    find: str | None = None,
    replace: str | None = None,
    find_file: Path | str | None = None,
    replace_file: Path | str | None = None,
    expected_source_sha256: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_patch_body(
        page,
        find=find,
        replace=replace,
        find_file=find_file,
        replace_file=replace_file,
        expected_source_sha256=expected_source_sha256,
    )


def wiki_page_delete(runtime_home: Path, page: str, *, mode: str = "tombstone", binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_delete(page, mode=mode)


def wiki_page_restore(runtime_home: Path, page: str, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_restore(page)


def wiki_publish_status(runtime_home: Path, *, binary: Path | str | None = None) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).publish_status()


def wiki_publish(
    runtime_home: Path,
    *,
    binary: Path | str | None = None,
    wiki_engine: Path | str | None = None,
    trigger: str = "agent",
    force: bool = False,
    node: str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).publish(
        wiki_engine=wiki_engine,
        trigger=trigger,
        force=force,
        node=node,
    )


def wiki_agent_identify(
    runtime_home: Path,
    *,
    thread_id: str,
    roles: list[str] | tuple[str, ...] = (),
    capabilities: list[str] | tuple[str, ...] = (),
    ttl_seconds: int = 1800,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_identify(
        thread_id=thread_id,
        roles=roles,
        capabilities=capabilities,
        ttl_seconds=ttl_seconds,
    )


def wiki_agent_register(
    runtime_home: Path,
    *,
    thread_id: str,
    roles: list[str] | tuple[str, ...] = (),
    capabilities: list[str] | tuple[str, ...] = (),
    ttl_seconds: int = 1800,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_register(
        thread_id=thread_id,
        roles=roles,
        capabilities=capabilities,
        ttl_seconds=ttl_seconds,
    )


def wiki_agent_heartbeat(
    runtime_home: Path,
    agent_id: str,
    *,
    ttl_seconds: int = 1800,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_heartbeat(
        agent_id,
        ttl_seconds=ttl_seconds,
    )


def wiki_agent_retire(
    runtime_home: Path,
    agent_id: str,
    *,
    reason: str = "completed",
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_retire(
        agent_id,
        reason=reason,
    )


def wiki_agent_whoami(
    runtime_home: Path,
    *,
    thread_id: str | None = None,
    agent_id: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_whoami(
        thread_id=thread_id,
        agent_id=agent_id,
    )


def wiki_agent_list(
    runtime_home: Path,
    *,
    include_stale: bool = False,
    include_retired: bool = False,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_list(
        include_stale=include_stale,
        include_retired=include_retired,
    )


def wiki_agent_status(
    runtime_home: Path,
    agent_id: str,
    *,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_status(agent_id)


def wiki_agent_inbox(
    runtime_home: Path,
    agent_id: str,
    *,
    include_archived: bool = False,
    include_snoozed: bool = False,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_inbox(
        agent_id,
        include_archived=include_archived,
        include_snoozed=include_snoozed,
    )


def wiki_agent_claim(
    runtime_home: Path,
    agent_id: str,
    message_id: str,
    *,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).agent_claim(agent_id, message_id)


def wiki_talk_append(
    runtime_home: Path,
    *,
    page: str,
    subject: str,
    from_address: str,
    to: list[str] | tuple[str, ...],
    body_markdown: str | None = None,
    body_file: Path | str | None = None,
    kind: str = "proposal",
    cc: list[str] | tuple[str, ...] = (),
    to_roles: list[str] | tuple[str, ...] = (),
    cc_roles: list[str] | tuple[str, ...] = (),
    attachments: list[Path | str] | tuple[Path | str, ...] = (),
    allow_tombstoned: bool = False,
    thread_id: str | None = None,
    reply_to: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).talk_append(
        page=page,
        subject=subject,
        from_address=from_address,
        to=to,
        body_markdown=body_markdown,
        body_file=body_file,
        kind=kind,
        cc=cc,
        to_roles=to_roles,
        cc_roles=cc_roles,
        attachments=attachments,
        allow_tombstoned=allow_tombstoned,
        thread_id=thread_id,
        reply_to=reply_to,
    )


def wiki_mail_inbox(
    runtime_home: Path,
    recipient: str,
    *,
    include_archived: bool = False,
    include_snoozed: bool = False,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_inbox(
        recipient,
        include_archived=include_archived,
        include_snoozed=include_snoozed,
    )


def wiki_mail_read(
    runtime_home: Path,
    *,
    message_id: str | None = None,
    thread_id: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_read(
        message_id=message_id,
        thread_id=thread_id,
    )


def wiki_mail_subscribe(
    runtime_home: Path,
    *,
    agent_id: str,
    address: str,
    relation: str = "subscriber",
    kinds: list[str] | tuple[str, ...] = (),
    ttl_seconds: int = 1800,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_subscribe(
        agent_id=agent_id,
        address=address,
        relation=relation,
        kinds=kinds,
        ttl_seconds=ttl_seconds,
    )


def wiki_mail_subscriptions(
    runtime_home: Path,
    *,
    agent_id: str | None = None,
    address: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_subscriptions(
        agent_id=agent_id,
        address=address,
    )


def wiki_mail_unsubscribe(
    runtime_home: Path,
    *,
    agent_id: str,
    address: str,
    relation: str | None = None,
    kinds: list[str] | tuple[str, ...] = (),
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_unsubscribe(
        agent_id=agent_id,
        address=address,
        relation=relation,
        kinds=kinds,
    )


def wiki_page_watch(
    runtime_home: Path,
    page: str,
    *,
    agent_id: str,
    list_address: str | None = None,
    kinds: list[str] | tuple[str, ...] = (),
    ttl_seconds: int = 1800,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_watch(
        page,
        agent_id=agent_id,
        list_address=list_address,
        kinds=kinds,
        ttl_seconds=ttl_seconds,
    )


def wiki_page_unwatch(
    runtime_home: Path,
    page: str,
    *,
    agent_id: str,
    list_address: str | None = None,
    kinds: list[str] | tuple[str, ...] = (),
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_unwatch(
        page,
        agent_id=agent_id,
        list_address=list_address,
        kinds=kinds,
    )


def wiki_page_assign_role(
    runtime_home: Path,
    page: str,
    *,
    agent_id: str,
    role: str,
    kinds: list[str] | tuple[str, ...] = (),
    ttl_seconds: int = 1800,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).page_assign_role(
        page,
        agent_id=agent_id,
        role=role,
        kinds=kinds,
        ttl_seconds=ttl_seconds,
    )


def wiki_list_create(
    runtime_home: Path,
    *,
    address: str,
    title: str | None = None,
    description: str | None = None,
    page: str | None = None,
    owner: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).list_create(
        address=address,
        title=title,
        description=description,
        page=page,
        owner=owner,
    )


def wiki_lists(
    runtime_home: Path,
    *,
    page: str | None = None,
    address: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).lists(
        page=page,
        address=address,
    )


def wiki_list_status(
    runtime_home: Path,
    address: str,
    *,
    include_archived: bool = False,
    include_snoozed: bool = False,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).list_status(
        address,
        include_archived=include_archived,
        include_snoozed=include_snoozed,
    )


def wiki_list_members(
    runtime_home: Path,
    address: str,
    *,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).list_members(address)


def wiki_mail_mark(
    runtime_home: Path,
    message_id: str,
    *,
    recipient: str,
    state: str,
    until: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_mark(
        message_id,
        recipient=recipient,
        state=state,
        until=until,
    )


def wiki_mail_claim(
    runtime_home: Path,
    message_id: str,
    *,
    recipient: str,
    agent_id: str,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_claim(
        message_id,
        recipient=recipient,
        agent_id=agent_id,
    )


def wiki_mail_mark_all(
    runtime_home: Path,
    message_id: str,
    *,
    state: str,
    until: str | None = None,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).mail_mark_all(
        message_id,
        state=state,
        until=until,
    )


def wiki_notify_poll(
    runtime_home: Path,
    agent_id: str,
    *,
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).notify_poll(agent_id)


def wiki_notify_ack(
    runtime_home: Path,
    notification_id: str,
    *,
    agent_id: str,
    state: str = "delivered",
    binary: Path | str | None = None,
) -> dict[str, Any]:
    return WikiCoreClient(runtime_home=runtime_home, binary=binary).notify_ack(
        notification_id,
        agent_id=agent_id,
        state=state,
    )


def _existing_binary(path: Path) -> Path:
    if not path.is_file():
        raise WikiCoreError(f"onecontext-wiki binary does not exist: {path}")
    return path


def _extend_text_or_file_arg(
    args: list[str],
    *,
    operation: str,
    inline_flag: str,
    inline_value: str | None,
    file_flag: str,
    file_value: Path | str | None,
) -> None:
    if inline_value is not None and file_value is not None:
        raise ValueError(f"{operation} accepts either {inline_flag} or {file_flag}, not both")
    if inline_value is None and file_value is None:
        raise ValueError(f"{operation} requires {inline_flag} or {file_flag}")
    if inline_value is not None:
        args.extend([inline_flag, inline_value])
        return
    args.extend([file_flag, str(file_value)])


def _dev_binary_is_stale(repo_root: Path, binary: Path) -> bool:
    if not binary.is_file():
        return True
    binary_mtime = binary.stat().st_mtime
    return any(path.stat().st_mtime > binary_mtime for path in (repo_root / "crates").rglob("*.rs"))


def _build_dev_binary(repo_root: Path) -> None:
    result = subprocess.run(
        ["cargo", "build", "--package", "onecontext-wiki-daemon"],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
        timeout=120,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or f"exit {result.returncode}"
        raise WikiCoreError(f"could not build onecontext-wiki dev binary: {detail}")


def _find_repo_root() -> Path:
    here = Path(__file__).resolve()
    for parent in here.parents:
        if (parent / "Cargo.toml").is_file():
            return parent
    return Path.cwd()

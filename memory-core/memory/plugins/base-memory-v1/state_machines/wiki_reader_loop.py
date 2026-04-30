from __future__ import annotations

from onectx.state_machines.v0_1 import Machine, emit, event, expect, sequence, step


def build() -> Machine:
    machine = Machine(
        "wiki_reader_loop",
        version="0.1.0",
        title="Wiki Reader Loop",
        description=(
            "Deterministic reader-side wiki preparation after agents have written "
            "talk entries, article proposals, concept pages, and redaction artifacts. "
            "This is deliberately not an agent metrology system: it makes the wiki "
            "navigable from existing state."
        ),
    )

    wiki = machine.scope(
        "wiki",
        key="workspace",
        states=["dirty", "building_inputs", "staged", "rendering", "rendered", "blocked"],
        initial="dirty",
        description="One markdown wiki workspace plus its concept-page directory.",
    )

    machine.clock("filesystem", source="workspace_file_changes")
    machine.clock("ledger", source="append_only_events")
    machine.clock("human", source="operator_trigger")

    machine.artifact(
        "topics_index",
        kind="markdown_page",
        path="{workspace}/topics.md",
        schema="wiki_index.v1",
        policies=["deterministic", "generated"],
        description="Concept pages grouped by frontmatter categories.",
    )
    machine.artifact(
        "projects_index",
        kind="markdown_page",
        path="{workspace}/projects.md",
        schema="wiki_index.v1",
        policies=["deterministic", "generated"],
        description="Project concept pages grouped by subject-type and project-status.",
    )
    machine.artifact(
        "open_questions_worklist",
        kind="markdown_page",
        path="{workspace}/open-questions.md",
        schema="wiki_worklist.v1",
        policies=["deterministic", "generated"],
        description="All open questions from concept pages and For You day-sections.",
    )
    machine.artifact(
        "resolved_staging_tree",
        kind="markdown_tree",
        path="{staging}",
        schema="wiki_staging_tree.v1",
        policies=["deterministic", "ephemeral"],
        description=(
            "A render staging tree where bracket links are resolved. Source prose "
            "does not need to be overwritten for links to work."
        ),
    )
    machine.artifact(
        "backlinks_index",
        kind="json",
        path="{staging}/_backlinks.json",
        schema="wiki_backlinks.v1",
        policies=["deterministic", "generated"],
        description="Inverted concept-link index used for What links here.",
    )
    machine.artifact(
        "staged_concept_pages",
        kind="markdown_tree",
        path="{staging}/concept",
        schema="wiki_concept_staging.v1",
        policies=["deterministic", "generated"],
        description="Concept pages wrapped with renderer frontmatter and backlinks.",
    )
    machine.artifact(
        "landing_page",
        kind="markdown_page",
        path="{workspace}/index.md",
        schema="wiki_landing.v1",
        policies=["deterministic", "generated"],
        description="The front door: start-here links, counts, and most-cited concepts.",
    )
    machine.artifact(
        "this_week_digest",
        kind="markdown_page",
        path="{workspace}/this-week.md",
        schema="wiki_digest.v1",
        policies=["deterministic", "generated"],
        description="Recent-changes style digest from talk folder decisions and wiki motion.",
    )
    machine.artifact(
        "render_manifest",
        kind="json_manifest",
        path="{wiki_family}/generated/render-manifest.json",
        schema="wiki_render_manifest.v1",
        policies=["deterministic", "generated", "route_table_source"],
        description=(
            "The wiki-engine render manifest for one page family. It records source "
            "inputs, generated files, localhost routes, renderer version, and theme assets."
        ),
    )
    machine.artifact(
        "wiki_route_table",
        kind="json_manifest",
        path="{wiki}/generated/site-manifest.json",
        schema="wiki_site_manifest.v1",
        policies=["deterministic", "generated", "browser_surface"],
        description="The localhost route table and content index used by `1context wiki serve`.",
    )

    machine.evidence(
        "wiki_inputs.ready",
        requires=[
            "topics_index.exists",
            "projects_index.exists",
            "open_questions_worklist.exists",
            "backlinks_index.exists",
            "staged_concept_pages.exist",
            "landing_page.exists",
            "this_week_digest.exists",
        ],
        description="Reader-side wiki inputs are ready for a renderer or static export.",
    )
    machine.evidence(
        "reader_loop.coherent",
        artifact="backlinks_index",
        checks=[
            "aliases resolve before external fallbacks",
            "unknown brackets degrade to plain text",
            "source markdown is not clobbered by staging-only bracket resolution",
            "concept pages expose What links here from backlinks index",
            "open questions remain first-class worklist entries",
        ],
        description="The wiki has outbound links, inbound backlinks, generated indexes, and a worklist.",
    )
    machine.evidence(
        "wiki_render.ready",
        artifact="render_manifest",
        checks=[
            "wiki.render.succeeded exists for every rendered family",
            "wiki.manifest.recorded exists for every rendered family",
            "wiki.generated.available exists for every rendered family",
            "site manifest and content index are refreshed after family renders",
            "localhost routes resolve from render manifests without requiring agent context",
        ],
        description="The wiki engine has turned source/talk folders into visible reader pages.",
    )

    machine.from_(wiki, "dirty").on(event("wiki.inputs.requested")).to(
        wiki,
        "building_inputs",
        do=sequence(
            step("build_topics_index"),
            step("build_projects_index"),
            step("build_open_questions"),
            step("resolve_brackets_to_staging"),
            step("build_landing_and_this_week"),
            step("resolve_brackets_to_staging"),
            step("stage_concepts_with_backlinks"),
            expect("wiki_inputs.ready"),
            expect("reader_loop.coherent"),
            emit("wiki.inputs.ready"),
        ),
    )

    machine.from_(wiki, "building_inputs").on(event("wiki.inputs.ready")).to(
        wiki,
        "rendering",
        do=sequence(
            step("run_wiki_engine_render"),
            step("write_site_manifest_and_content_index"),
            expect("wiki_render.ready"),
            emit("wiki.render.ready"),
        ),
    )

    machine.from_(wiki, "rendering").on(event("wiki.render.ready")).to(wiki, "rendered")

    return machine

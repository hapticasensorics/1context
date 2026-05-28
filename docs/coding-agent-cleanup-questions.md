# Coding Agent Cleanup Questions

For each answer, include:

- paths inspected
- evidence
- proposed edit
- risk
- validation command

No vibes, no "seems like." The project is pre-release, so the default posture
is deletion of legacy, migration, scaffold, fallback, compatibility, repair,
backfill, alias, and synthetic proof paths.

## Repo State And Current Truth

1. What is the current must-pass verification set for the repo?
2. What files in `scripts/` are referenced by CI, package scripts, release
   flows, docs, or other scripts?
3. What are the top 25 largest files in `scripts/`, by line count, and what
   unique behavior does each claim to verify?
4. Which scripts are pure dogfood, demo, or proof generation rather than
   product verification?
5. Which scripts write checked-in or persistent evidence artifacts?
6. What currently depends on `scripts/release-train.sh`?
7. What scripts are safe to delete immediately because they have no active
   references and no unique current-contract coverage?
8. Which stale words appear in active product code, tests, and docs:
   `legacy`, `compat`, `compatibility`, `migration`, `fallback`, `scaffold`,
   `repair`, `alias`, `backfill`, `upgrade`?

## Memory DB And Data Model

9. What is the current intended memory DB schema?
10. Do active tests require migration behavior, dirty flags, repair SQL,
    checksum backfill, migrate-twice behavior, or live reapply behavior?
11. Are there any real external users, deployed DBs, or customer-like data
    stores that would make migration deletion unsafe?
12. What does `crates/onecontext-memory-db/src/db.rs` accept as database
    configuration?
13. Where does `query_density.rs` use raw-table fallback or schema ambiguity?
14. What exact source formats do `codex_agent_ingest.rs` and
    `claude_agent_ingest.rs` accept?
15. What would a fresh empty-dev/test DB bootstrap look like with no migration
    runner?

## Contracts And Typed Schemas

16. Which JSON payloads are parsed with `serde_json::Value` helper ladders in
    Rust?
17. Which contracts are duplicated across Rust, Swift, JS, scripts, and docs?
18. Should `onecontext-contracts` exist now, or should schemas first live
    inside existing crates?
19. Where are timestamps manually parsed or string-sliced?
20. Where are CLI args manually parsed in Rust and Swift?

## Wiki, Parser, And Engine Cleanup

21. What command aliases or string-matched errors exist in
    `onecontext-wiki-daemon`?
22. Where does wiki core mutate TOML by string manipulation?
23. Where are markdown links, citations, sections, frontmatter, or slugs parsed
    by regex or substring scanning?
24. What is the current canonical deletion model for wiki pages: hard delete,
    archive, tombstone, or restore lifecycle?
25. What are the active wiki routes, and what legacy `.talk` aliases,
    generated talk stubs, fallback route behavior, and tests preserve them?

## Capture, Attention, App, Release, And Devtools

26. What is the current READY bundle contract for capture/attention?
27. Where does capture manufacture inferred lanes from old windows data?
28. Is `onecontext-capture-dashboard` product, devtool, or archive?
29. Which release proofs are real measurements, and which are synthetic fixture
    proofs generated from expectations?
30. What is the minimal final validation matrix after cleanup, including exact
    commands and pass conditions for Rust, Swift, wiki engine, release runner,
    Playwright, macOS diagnose, scripts size gate, and stale-reference gate?

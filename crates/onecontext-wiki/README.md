# OneContext Wiki

This directory groups the wiki runtime crates under one system boundary.

## Layout

- `core/` contains portable wiki semantics: page lifecycle, source writes,
  talk, assets, validation, publish preflight, renderer invocation, and receipts.
- `cli/` contains the thin JSON/CLI adapter for calling the core from the app,
  scripts, and local proof workflows.

The crate package names remain `onecontext-wiki-core` and
`onecontext-wiki-daemon` for now so existing build and test commands keep
working while the filesystem layout becomes clearer.

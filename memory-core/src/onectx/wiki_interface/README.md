# Wiki Interface

This folder is the Python boundary for wiki-facing memory work.

Memory code may call this package to:

- write route-plan, talk, proposal, decision, preview, and receipt records
- promote an accepted source edit into `1Context/user-wiki/source`
- ask the Swift daemon for `wiki.refresh`

This folder does not own:

- site-map materialization from `wiki.toml`
- the JavaScript renderer
- Swift render queue, publication, or last-good serving
- local web APIs
- hired-agent planning or execution
- installed runtime defaults

Those behaviors live in `runtime/`, `wiki-engine/`, and `macos/Sources`.

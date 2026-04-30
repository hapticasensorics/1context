# Wiki Reader Loop IR Diagram

## How to read it

This diagram is generated from the compiled `wiki_reader_loop` state-machine IR.

The reader loop now has two deterministic phases:

- build reader inputs: topics, projects, open questions, backlinks, landing,
  this-week, bracket staging, and staged concept pages
- render browser surface: wiki-engine family render, render manifest, site
  manifest, content index, and localhost route table

The evidence gate is no longer just "markdown inputs exist." The loop expects
the renderer/browser subsystem to leave `wiki.render.succeeded`,
`wiki.manifest.recorded`, and `wiki.generated.available` evidence before the
wiki is considered rendered.

```mermaid
%% See wiki-reader-loop.ir.mmd for generated source.
```

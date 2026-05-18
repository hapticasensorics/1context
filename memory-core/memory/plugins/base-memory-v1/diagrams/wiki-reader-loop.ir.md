# Wiki Reader Loop IR Diagram

## How to read it

This diagram is generated from the compiled `wiki_reader_loop` state-machine IR.

The reader loop now has two deterministic phases:

- build reader inputs: topics, projects, open questions, backlinks, landing,
  this-week, bracket staging, and staged concept pages
- request browser publication: write a `wiki.refresh` request for the Swift
  render queue

The evidence gate is no longer "memory-core rendered the browser surface." The
loop expects `wiki.refresh.requested`; the renderer/browser subsystem proves
routes, markdown twins, and manifests in the Swift/wiki-engine runtime.

```mermaid
%% See wiki-reader-loop.ir.mmd for generated source.
```

# Wiki Growth Fabric IR Diagram

## How to read it

This diagram is generated from the compiled `wiki_growth_fabric` state-machine IR.

Use it as the honest map of what the DSL currently represents:

- `idle -> scanning -> routing -> running_agents -> building_reader_surface -> review_ready`
- `routing` fans out into dynamic hired-agent jobs from `role_route_plan.*`
- concurrency is expressed as `runtime_policy.max_concurrent_agents`
- the corpus can retrigger the fabric from `corpus.changed` or `roles.need_reconfiguration`
- evidence gates are visible as `expect:` labels
- `building_reader_surface` now invokes both deterministic input building and
  wiki-engine rendering, with render-manifest evidence as the browser-ready gate

The more narrative `wiki-growth-fabric.mmd` explains the intended system. This
IR diagram is deliberately stricter: if it is missing a behavior, the compiled
DSL does not yet represent that behavior clearly enough.

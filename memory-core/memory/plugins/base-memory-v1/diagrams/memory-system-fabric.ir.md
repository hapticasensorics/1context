# Memory System Fabric IR

## How to read it

This diagram is generated from the compiled `memory_system_fabric` state-machine
IR. It is the stricter version of the narrative `memory-system-fabric.mmd`.

The main cycle is:

- `idle`
- `ingesting`
- `planning`
- `rendering_experience`
- `birthing_agents`
- `running_agents`
- `validating`
- `routing_wiki`
- `building_reader_surface`
- `complete`

The important correction from the earlier wiki-only diagram is that lived
experience and hired-agent birth are now first-class states in the system loop.
`wiki_growth_fabric` appears here as a child submachine invoked after agent
outputs settle, not as the whole memory system.

`building_reader_surface` now means the deterministic wiki input pass plus the
wiki-engine render pass. The evidence expected at that point includes
`wiki.render.succeeded`, `wiki.manifest.recorded`, and
`wiki.generated.available`, so the top-level memory cycle closes on a visible
reader surface rather than only on intermediate markdown staging.

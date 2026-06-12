# Typed Society DSL

Status: greenfield design review draft  
Purpose: define the smallest useful state-machine language for agent societies.

## The Bet

The language should be almost embarrassingly simple.

Conway's Game of Life does not encode gliders, oscillators, or computation.
It gives a tiny world with tiny rules, then lets complexity emerge from
experience.

1Context should do the same for agent organizations.

```text
tiny social rules + durable memory + lived transcript
```

The DSL should define the social physics of the organization. Agents discover
the work.

## Essence

1Context is a small society for agents.

Agents are hired into a society with roles, places, state, moves, memory, and
boundaries. They act inside that structure. Their actions create experience.
Future agents read the society file and the transcript, then become more capable
inside the same world.

The center:

```text
structure first, adaptation second, transcript always
```

The society file is the genome.  
The transcript is lived experience.  
The implementation is just the body.

## The Whole Contract

The core language should fit on one screen:

```text
society  a named agent organization
role     a standing social function
place    where work, discussion, memory, or observation happens
state    shared vocabulary for a thing's condition
move     an allowed social act that may change state or memory
memory   durable record of artifacts, proposals, evidence, and history
boundary what a role may not read, write, spend, publish, or use
```

That is the proposed core.

Everything else should be either:

- a profile binding,
- a Pydantic schema,
- a runtime implementation detail,
- or a pattern that emerges through experience.

## Tiny Example

```text
society wiki_memory

places
  memory      binds wiki.pages
  discussion  binds wiki.talk
  experience  binds lakestore

roles
  librarian leads memory
  curator works_in memory
  scout watches experience

state proposal
  draft
  discussing
  adopted
  rejected

moves proposal
  open    draft      -> discussing by any
  adopt   discussing -> adopted    by librarian
  reject  discussing -> rejected   by librarian

memory
  transcript records all moves
  evidence required for adopt

boundaries
  private_context read by librarian
```

A future agent should be able to read that and understand the society it has
entered in seconds.

## Primitive 1: `society`

A `society` is the whole living organization.

It has a name, purpose, roles, places, moves, memory, and boundaries. It is not
a workflow. It is the small institution agents join.

```text
society wiki_memory
```

Design rule: the society should be stable by default. It can adapt, but it
should not constantly reinvent itself.

## Primitive 2: `role`

A `role` is a standing social function.

Roles carry jurisdiction, taste, duty, and authority. A role is more than a
permission set. A role says what kind of judgment the society expects.

```text
roles
  librarian leads memory
  curator works_in memory
  verifier checks evidence
```

Roles are how agents are hired into the society.

## Primitive 3: `place`

A `place` is where something happens.

Places are abstract. They can bind to wiki pages, talk pages, files, issues,
Slack threads, databases, local folders, or future substrates we do not know
yet.

```text
places
  memory      binds wiki.pages
  discussion  binds wiki.talk
  experience  binds lakestore
```

The universal DSL says `place`. A machine profile says what the place binds to.

This avoids brittle language like `talk_page` or `lakestore` in the core.

## Primitive 4: `state`

`state` is shared vocabulary.

It helps agents coordinate around the condition of a proposal, job, place, or
role. It should stay lightweight.

```text
state proposal
  draft
  discussing
  adopted
  rejected
```

State is not a prison. It is a common language for coordination.

## Primitive 5: `move`

A `move` is an allowed social act.

This is the heart of the language. "Transition" sounds mechanical. "Move" is
agent-shaped.

Moves can:

- change state
- write memory
- open discussion
- ask a role to act
- adopt or reject a proposal
- publish an artifact
- reopen work
- forget or archive something

```text
moves proposal
  open    draft      -> discussing by any
  adopt   discussing -> adopted    by librarian
  reject  discussing -> rejected   by librarian
```

Design rule: moves should be few, readable, and socially meaningful.

## Primitive 6: `memory`

`memory` is the durable record.

It includes artifacts, proposals, evidence, commitments, decisions, and the
operation transcript.

```text
memory
  transcript records all moves
  evidence required for adopt
```

The transcript is not a debug log. It is the lived experience of the society.
Future agents read it to understand how the current structure came to be.

## Primitive 7: `boundary`

A `boundary` is a hard limit around resources.

Boundaries protect secrets, write authority, budget, publication, destructive
tools, and other real-world risk. Boundaries do not constrain imagination.

```text
boundaries
  private_context read by librarian
  source_pages write by curator,librarian
  publish_web by librarian
```

Design rule: if it limits access, it is a boundary. If it limits thought, it is
wrong.

## What About Proposals, Evidence, And Commitments?

They are important, but they do not need to be top-level primitives yet.

They can be represented as memory plus moves:

- `proposal`: memory item created by a move
- `evidence`: memory item required by a move
- `commitment`: memory item created when a proposal is adopted
- `transcript`: memory stream recording moves and their reasons

This keeps the core small.

If experience shows one of these deserves first-class syntax, the society can
evolve.

## What Pydantic Does

Pydantic makes the tiny language solid.

It validates:

- society files
- roles
- places and bindings
- state names
- move source/target states
- allowed actors
- boundary rules
- memory references
- transcript entries
- profile bindings

Pydantic should not make decisions. It should make records coherent.

## How This Progresses

### Stage 0: One Readable Society File

Write one society file for `wiki_memory`.

It should be readable by a human or future agent without implementation
knowledge.

### Stage 1: Pydantic Schema

Define the smallest Pydantic models:

```text
SocietyModel
RoleModel
PlaceModel
StateModel
MoveModel
MemoryRuleModel
BoundaryModel
ProfileBindingModel
TranscriptEntryModel
```

Reject malformed structure. Do not add execution yet.

### Stage 2: Compile To Current Runtime

Compile the society file into today's state-machine IR or a tiny successor IR.

The runtime can stay boring. The authored file stays beautiful.

### Stage 3: Transcript First Execution

Every move writes a transcript entry:

```text
actor
role
move
from_state
to_state
place
reason
evidence
timestamp
```

The transcript becomes the experience stream for future agents.

### Stage 4: Let Experience Shape Structure

Agents can propose changes to the society file:

- add a role
- rename a move
- split a place
- add a boundary
- change adoption authority

But changes happen through moves and transcript, not hidden mutation.

## Example Machine: Wiki Memory

This is the first living pattern inside the tiny rule world.

It should be obvious at a glance:

```text
scribes witness experience
biographer finds continuity
biographer asks scribes questions
scribes answer from local experience
biographer proposes biography, projects, topics, and questions
curators adopt, revise, reject, or request more evidence
the transcript teaches the next generation of agents
```

This pattern should not become a rigid 24-scribes-then-biographer pipeline.
The society names the roles and moves. Agents use experience to discover how
many scribes are needed, which questions matter, what concepts deserve pages,
and how the day flows into the last two weeks.

```text
society wiki_memory
purpose "maintain a durable personal context wiki through agent collaboration"

places
  memory      "durable memory surface"      binds wiki.pages
  discussion  "proposal and decision forum" binds wiki.talk
  experience  "queryable lived context"     binds lakestore

roles
  scribe      witnesses experience
  biographer composes continuity
  curator    works_in memory
  librarian  leads memory
  scout      watches experience
  verifier   checks memory

state proposal
  draft
  discussing
  adopted
  rejected
  deferred

state work
  queued
  active
  waiting_evidence
  done
  failed

state memory_thread
  witnessed
  questioned
  answered
  synthesized
  proposed
  adopted

moves proposal
  open     draft      -> discussing by any       at discussion
  adopt    discussing -> adopted    by librarian at discussion requires evidence
  reject   discussing -> rejected   by librarian at discussion
  defer    discussing -> deferred   by librarian at discussion

moves work
  start    queued           -> active           by assigned_role
  submit   active           -> waiting_evidence by assigned_role
  accept   waiting_evidence -> done             by verifier requires evidence
  fail     active           -> failed           by assigned_role

moves memory_thread
  witness    *            -> witnessed   by scribe      at experience
  ask        witnessed    -> questioned  by biographer  at discussion
  answer     questioned   -> answered    by scribe      at discussion
  synthesize answered     -> synthesized by biographer  at memory
  propose    synthesized  -> proposed    by biographer  at discussion
  adopt      proposed     -> adopted     by curator     at memory requires evidence

memory
  transcript records all moves
  evidence required for adopt,accept,synthesize
  proposals live in discussion
  scribe memories live in experience
  biographies live in memory

boundaries
  private_context read by librarian,curator
  memory write by curator,librarian,biographer
  structure adopt by librarian
```

The old draconian version would encode the whole pipeline.

The society version encodes social physics:

- who can witness
- who can ask
- who can answer
- who can synthesize
- who can propose
- who can adopt
- where memory lives
- what must be evidenced
- what gets written to transcript

The bitter lesson lives here: do not over-hand-code cognition into the
controller. Give agents roles, places, moves, memory, feedback, and transcript.
Let experience do the heavy lifting.

## Engineering Notes

The implementation can be sophisticated while the language stays simple.

Under the hood we can still have:

- Pydantic models
- JSON Schema export
- adapters for wiki/talk/lakestore
- current state-machine runtime compatibility
- queue and supervision
- evidence validators
- transcript storage

But none of those should pollute the society file.

## Design Tests

The proposal is good if:

- the society file is readable in one sitting
- a future agent can infer how to participate
- concrete systems are profile bindings, not core primitives
- boundaries protect resources without limiting creativity
- every move can create transcript experience
- the language is small enough to remember
- complexity emerges from agents acting over time

The proposal is bad if:

- the core DSL starts naming wiki-specific things
- every concept becomes first-class too early
- state becomes a cage
- self-modification becomes constant churn
- Pydantic becomes the philosophy
- the transcript is treated like logs

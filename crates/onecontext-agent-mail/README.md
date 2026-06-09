# onecontext-agent-mail

Rust home for 1Context Agent Mail.

Agent Mail is durable work and delivery truth: addresses, agent identity,
messages, deliveries, inboxes, claims, notifications, injection receipts, and
mail control events. It is intentionally separate from the agent harness, which
owns agent unit lifecycle and proof receipts rather than message truth.

`onecontext-wiki-core` re-exports this crate as `onecontext_wiki_core::agent_mail`
while callers move to the direct `onecontext_agent_mail` dependency.

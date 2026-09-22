# Korri product build

The canonical requirements are in [the product spec](../consistent-korri-product/spec.md). The glossary in [`CONTEXT.md`](../../CONTEXT.md) defines the product terms used by these tickets.

Work uses a continuous dependency queue. Start a ticket when its direct blockers have a stable integration commit. Do not wait for unrelated tickets.

The conductor owns integration branches, shared files, ticket state, reviews, and broad validation. Workers own one observable behavior, an explicit file set, one focused verification command, and one commit boundary.

Two changes require clean-cut integration branches:

- Tickets 01 and 08 through 11 form the product-module cut. Keep them off `main` until every exported device passes the product check without exceptions.
- Ticket 15 forms the streaming-host plugin cut. Land the plugin and deletion of the shared host composition together.

Tickets 12, 13, and 14 run from their direct Phase 1 foundations. They do not wait for the product-module cut. Tickets 16 through 18 start when their own `Blocked by` lines permit.

Workers run focused checks. The conductor runs affected suites on integrated checkpoints and the full required validation before each clean cut lands. Hardware checks block only behavior that software cannot observe.

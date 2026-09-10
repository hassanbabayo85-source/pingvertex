# Scheduler Backpressure Controls

The durable scheduler uses layered capacity controls. Operational values are
configuration, not Free-tier business constants.

- `WORKER_MAX_CONCURRENT_CHECKS`: global execution concurrency.
- `SCHEDULER_MAX_CONCURRENCY_PER_TENANT`: maximum active leased monitor checks
  for one tenant across scheduler instances.
- `SCHEDULER_DUE_CLAIM_LIMIT_PER_TENANT`: maximum due monitors claimed for one
  tenant in a claim cycle.
- `SCHEDULER_DETERMINISTIC_SPREAD_SECONDS`: bounded deterministic delay added
  after a completed claim to spread monitors that share the same due timestamp.
- `SCHEDULER_JITTER_MS`: randomized claim-cycle and retry jitter.
- `SCHEDULER_EXECUTION_RETRY_LIMIT` and
  `SCHEDULER_EXECUTION_RETRY_BASE_MS`: bounded retry/backoff controls.

The database claim path uses a transaction-scoped advisory lock per tenant,
counts currently leased work, and limits the selected due rank by the remaining
tenant capacity. This prevents multiple scheduler instances from independently
over-consuming the same tenant's concurrency budget.

The worker publishes:

- `scheduler_due_queue_depth`
- `scheduler_active_tasks`
- `scheduler_backpressure_total{reason="global"}`
- `scheduler_suspended_tenant_skips_total`
- `scheduler_entitlement_skips_total`

Free-tier monitor limits and minimum intervals are resolved through
`ResolveEntitlementsUseCase`; the scheduler does not contain Free-tier
business constants.


Regional agents use the same tenant caps at the mTLS claim boundary. Their
additional `SCHEDULER_AGENT_CLAIM_LIMIT` value controls the maximum number of
checks returned to one agent claim request. The database tenant semaphore is
still authoritative across agents and regions.


HTTP execution ownership is split to avoid duplicate checks and double billing:
single-region HTTP monitors remain on the durable worker scheduler, while
multi-region HTTP monitors are claimed per region by mTLS agents. The database
claim predicates enforce this split.

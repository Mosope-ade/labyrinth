# Worker boundary

Future separately runnable job worker. Invokes the engine, enforces computation
budgets, tracks attempts/leases and stages artifacts for atomic visibility. It must
not implement a second compiler. Supabase Queues/Postgres/Storage are selected (ADR 0003). Transactional admission
is implemented in the API; worker claim/execution remains pending.
Library scaffold only; no jobs are executed yet.

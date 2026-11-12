# Worker boundary

Future separately runnable job worker. Invokes the engine, enforces computation
budgets, tracks attempts/leases and stages artifacts for atomic visibility. It must
not implement a second compiler. Queue/database/object-store providers remain open.
Library scaffold only; no jobs are executed yet.

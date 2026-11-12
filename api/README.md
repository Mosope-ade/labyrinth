# API boundary

Future Axum application: authentication/ownership, intent clarification, project and
revision management, asynchronous job admission, status/events, private downloads and
admin operations. Library scaffold only; no server or endpoints are implemented.

`src/routes/`, `src/auth/`, `src/admin/`, `src/jobs/` and `src/storage/` reserve owning
boundaries. `migrations/` remains empty until the database is selected. AI parsing
and assessment adapters belong in `src/ai/`, outside the pure engine. Do not add a
synchronous alternate network generator here.

# TriageKit — agent notes

TriageKit is a small, self-hostable issue / feature-request inbox.

- `apps/server` — Rust (Axum) API: SSO, sessions (JWT), issues, user provisioning.
- `apps/web` — Vite + React single-page app (the triage UI).
- `docs/integration.md` — the SSO + provisioning contract for host apps.

## Data layer

Persistence lives behind the `db` module in the Rust server
(`apps/server/src/db/`). It supports two interchangeable backends, selected at
startup by the `DB_BACKEND` env var:

- `mongo` (default) — `apps/server/src/db/mongo.rs`
- `postgres` — `apps/server/src/db/postgres.rs`

`apps/server/src/db/mod.rs` holds the `Db` enum that dispatches to the active
backend and defines the method set both backends implement.

**When you add or change a persistence method, do it in three places:** the
dispatch arm in `db/mod.rs`, the MongoDB impl in `db/mongo.rs`, and the
PostgreSQL impl in `db/postgres.rs`. Keep their domain semantics identical
(conflict / not-found errors, upsert keyed by `externalUserId`, idempotent
bootstrap). There is no Convex in this project anymore.

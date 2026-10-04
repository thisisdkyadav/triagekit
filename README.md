# TriageKit

A small, self-hostable issue & feature-request inbox you can drop into any app.
Your app keeps owning login — it hands users to TriageKit over a short-lived SSO
code, and your backend can provision users over a simple API.

- **Board & list views** — triage by status, drag cards between columns
- **Bugs, features, and questions** in one inbox, with priorities
- **Drop-in SSO** — host apps mint a login code; no separate password for end users
- **User provisioning API** — create/update/delete the allow-list from your app
- **Developer accounts** for the people who triage

## Architecture

```
┌────────────┐    SSO login code     ┌──────────────┐
│  Host app  │ ────────────────────▶ │  Web (React) │
│  (e.g HMS) │ ◀──── provisioning ─┐ └──────┬───────┘
└────────────┘   (app-secret API) │        │ REST
                                   │        ▼
                                   │  ┌──────────────┐            ┌─────────────────────┐
                                   └─▶│  API (Rust)  │ ─────────▶ │  MongoDB _or_       │
                                      │  Axum + JWT  │  DB_BACKEND │  PostgreSQL (data)  │
                                      └──────────────┘            └─────────────────────┘
```

- **`apps/web`** — Vite + React single-page app (the triage UI).
- **`apps/server`** — Rust (Axum) API: SSO, sessions (JWT), issues, user provisioning.
  Persistence goes through a pluggable `db` module with two interchangeable
  backends, chosen by the `DB_BACKEND` env var: **`mongo`** (default) or
  **`postgres`**. Schema / indexes are created automatically on startup.
- SSO login codes live in memory by default, or Redis (`TOKEN_STORE=redis`).

## Quick start

**Prerequisites:** Node 18+, Rust (stable), and a database — **MongoDB**
(default) or **PostgreSQL** running locally.

```bash
# 1. Configure
cp .env.example .env        # defaults work for local dev (DB_BACKEND=mongo)

# 2. Install web deps
npm install
npm run install:web

# 3. Start your database (default backend is mongo)
#    mongod   # or run Postgres and set DB_BACKEND=postgres

# 4. Run the Rust API + web together
npm run dev
```

`npm run dev` launches the Rust API and the Vite dev server. On startup the API
connects to the configured database, creates its schema / indexes, and seeds a
first owner account. The web app is served at
`http://localhost:5173/issue-management`.

A first owner account is seeded on startup from `BOOTSTRAP_ADMIN_EMAIL` /
`BOOTSTRAP_ADMIN_PASSWORD` — use it to sign in at the web app.

### Run services individually

```bash
cd apps/server && cargo run          # API on :8080 (connects to the DB)
npm run dev:web                      # web on :5173
```

### Switching database backend

Set `DB_BACKEND` in `.env`:

```bash
DB_BACKEND=mongo       # MongoDB  (uses MONGO_URI / MONGO_DB)
DB_BACKEND=postgres    # Postgres (uses DATABASE_URL)
```

Both backends are created/migrated automatically on first run — no manual
schema step. The API contract is identical regardless of backend.

## Configuration

All settings come from `.env` (see `.env.example` for the annotated list).

| Variable | Default | Purpose |
| --- | --- | --- |
| `VITE_API_URL` | `http://localhost:8080` | API base URL the web app calls |
| `VITE_APP_BASE_PATH` | `/issue-management` | Base path the SPA is served under |
| `SERVER_HOST` / `SERVER_PORT` | `127.0.0.1` / `8080` | API bind address |
| `PUBLIC_WEB_URL` | `http://localhost:5173` | Web origin used to build SSO redirects |
| `TRIAGEKIT_APP_SECRET` | `dev-app-secret` | Bearer token host apps use for SSO + provisioning |
| `JWT_SECRET` | — | Signs TriageKit session tokens |
| `BOOTSTRAP_ADMIN_EMAIL` / `_PASSWORD` | `dev@example.com` / … | Seed owner account |
| `TOKEN_STORE` | `memory` | `memory` or `redis` for SSO login codes |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Used only when `TOKEN_STORE=redis` |
| `DB_BACKEND` | `mongo` | Data backend: `mongo` or `postgres` |
| `MONGO_URI` | `mongodb://127.0.0.1:27017` | MongoDB connection (when `DB_BACKEND=mongo`) |
| `MONGO_DB` | `triagekit` | MongoDB database name |
| `DATABASE_URL` | `postgres://postgres:postgres@127.0.0.1:5432/triagekit` | Postgres DSN (when `DB_BACKEND=postgres`) |

> Set strong values for `TRIAGEKIT_APP_SECRET` and `JWT_SECRET` in production,
> and point the database vars at your managed instance.

## Deploying (Linux + systemd + nginx)

One command on the server — **no Rust or Node needed**, it downloads a prebuilt
static binary and SPA from the latest GitHub release:

```bash
curl -fsSL https://raw.githubusercontent.com/thisisdkyadav/triagekit/main/scripts/install.sh | sudo bash
```

It installs the API to `/usr/local/bin`, writes `/etc/triagekit/triagekit.env`
with generated secrets, publishes the SPA to `/var/www/triagekit`, installs
`triagekit.service` (enabled + `Restart=always`), starts it on
`127.0.0.1:8080`, and prints the owner login once. Prefer to build locally?
`--from-source`.

> Prebuilt install needs a published release. Cut one by pushing a tag
> (`git tag v0.1.0 && git push origin v0.1.0`) — the release workflow builds
> the static binary + web bundle. Until then the installer falls back to
> building from source (needs Rust + Node).

Then edit the one config file and re-run to apply:

```bash
sudo nano /etc/triagekit/triagekit.env    # PUBLIC_WEB_URL, DB_BACKEND, ...
sudo /opt/triagekit/scripts/install.sh    # also the upgrade path
```

Serve the frontend by adding a path to nginx (snippet in
`deploy/nginx.conf.example`) — the SPA is static files in `/var/www/triagekit`
and `/api` proxies to the service:

```nginx
location /issue-management/ {
    alias /var/www/triagekit/;
    try_files $uri $uri/ /issue-management/index.html;
}
location /api/ {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host              $host;
    proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

```bash
systemctl status triagekit     # health
journalctl -u triagekit -f     # logs
```

Full guide, including PostgreSQL, root-domain hosting, upgrades and
uninstall: **[docs/deploy.md](docs/deploy.md)**.

## Integrating a host app

Your backend creates a short-lived login code, then redirects the user to the
returned URL — TriageKit exchanges it for a session automatically.

```bash
curl -X POST http://localhost:8080/api/sso/login-code \
  -H "Authorization: Bearer dev-app-secret" \
  -H "Content-Type: application/json" \
  -d '{
    "externalUserId": "hms-admin-1",
    "email": "admin@example.com",
    "name": "HMS Admin",
    "role": "admin",
    "returnTo": "/issues"
  }'
```

Your backend can also manage the user allow-list directly
(`POST/PATCH/DELETE /api/sso/users`). See **[docs/integration.md](docs/integration.md)**
for the full SSO and provisioning contract.

## API overview

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| `POST` | `/api/sso/login-code` | App secret | Mint a one-time SSO login code |
| `POST` | `/api/sso/exchange` | Login code | Exchange a code for a session |
| `POST` | `/api/sso/users` | App secret | Create one allowed user (`upsert` optional) |
| `POST` | `/api/sso/users/bulk` | App secret | Create/update many allowed users |
| `PATCH` | `/api/sso/users/{externalUserId}` | App secret | Edit an allowed user |
| `DELETE` | `/api/sso/users/{externalUserId}` | App secret | Remove an allowed user |
| `POST` | `/api/developer/login` | — | Developer email/password login |
| `GET/POST` | `/api/developers` | Session | List / create developer accounts |
| `GET/POST` | `/api/allowed-users` | Session | List / create allowed users |
| `GET/POST` | `/api/issues` | Session | List / create issues |
| `POST` | `/api/issues/{id}/status` | Session (developer) | Update an issue's status |

## Project structure

```
apps/
  web/               Vite + React single-page app
  server/            Rust (Axum) API
    src/db/          Pluggable data layer (mod.rs + mongo.rs + postgres.rs)
docs/                Integration contract
scripts/             Local dev orchestration (dev.sh)
```

## Contributing

Issues and pull requests are welcome. Please keep changes small and focused, and
run the build for any area you touch:

```bash
cd apps/server && cargo build      # Rust API
npm --prefix apps/web run build    # Web app
```

## License

[MIT](LICENSE) © 2026 Devesh Yadav

# Deploying TriageKit

Target: a Linux box with systemd and nginx. The API runs as a systemd service on
`127.0.0.1:8080`; nginx serves the built SPA from a path and proxies `/api` to
the API. One origin, so no CORS to think about.

## Prerequisites

- systemd Linux, root/sudo
- Rust (stable) and Node 18+ — used to build; not needed at runtime
- A database: **MongoDB** (default) or **PostgreSQL**
- nginx

```bash
# Rust, if missing
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## 1. Install

```bash
curl -fsSL https://raw.githubusercontent.com/thisisdkyadav/triagekit/main/scripts/install.sh | sudo bash
```

Or from a checkout:

```bash
git clone https://github.com/thisisdkyadav/triagekit.git
cd triagekit
sudo ./scripts/install.sh
```

The installer:

1. clones/updates the source into `/opt/triagekit`
2. creates the `triagekit` system user
3. writes `/etc/triagekit/triagekit.env` with **generated secrets** (never
   overwritten on re-run)
4. builds the API and installs it to `/usr/local/bin/triagekit-server`
5. builds the SPA and publishes it to `/var/www/triagekit`
6. installs + enables `triagekit.service` and starts it
7. prints the owner login and password **once**

> If Rust was installed via rustup as your normal user, root won't see `cargo`.
> Run `sudo -E env "PATH=$PATH" ./scripts/install.sh`.

## 2. Configure

Everything lives in one file: `/etc/triagekit/triagekit.env`.

```bash
sudo nano /etc/triagekit/triagekit.env
sudo ./scripts/install.sh      # re-run to apply (rebuilds web, restarts service)
```

At minimum set:

| Key | What |
| --- | --- |
| `PUBLIC_WEB_URL` | Public origin, e.g. `https://hms.example.com` (no trailing slash) |
| `APP_BASE_PATH` | Path nginx serves the SPA under, e.g. `/issue-management` |
| `DB_BACKEND` | `mongo` or `postgres` |
| `MONGO_URI` / `DATABASE_URL` | Connection string for the chosen backend |
| `BOOTSTRAP_ADMIN_EMAIL` | First owner account |

> **systemd env file format:** `KEY=value`, one per line. Trailing `# comments`
> are **not** stripped — they become part of the value. Keep comments on their
> own line and quote values containing spaces.

`VITE_API_URL` and `VITE_APP_BASE_PATH` are **baked into the JS bundle at build
time**. After changing either, re-run `install.sh` so the frontend is rebuilt.
Leave `VITE_API_URL` empty for the same-origin nginx setup below.

Changes that only affect the API (secrets, database, ports) need just:

```bash
sudo systemctl restart triagekit
```

## 3. nginx

Copy the snippet from `deploy/nginx.conf.example` into your site's `server`
block:

```nginx
location = /issue-management { return 301 /issue-management/; }

location /issue-management/ {
    alias /var/www/triagekit/;
    try_files $uri $uri/ /issue-management/index.html;
}

location /issue-management/assets/ {
    alias /var/www/triagekit/assets/;
    expires 1y;
    add_header Cache-Control "public, immutable";
}

location = /issue-management/index.html {
    alias /var/www/triagekit/index.html;
    add_header Cache-Control "no-store";
}

location /api/ {
    proxy_pass http://127.0.0.1:8080;
    proxy_http_version 1.1;
    proxy_set_header Host              $host;
    proxy_set_header X-Real-IP         $remote_addr;
    proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

```bash
sudo nginx -t && sudo systemctl reload nginx
```

Then open `https://your-host/issue-management/`.

### Serving at the domain root instead

Set `APP_BASE_PATH=/` and `VITE_APP_BASE_PATH=/`, re-run `install.sh`, and use
the standalone `server { }` block at the bottom of `nginx.conf.example`.

## 4. Operate

```bash
systemctl status triagekit        # is it up
systemctl restart triagekit       # apply API config changes
journalctl -u triagekit -f        # live logs
journalctl -u triagekit -n 100    # recent logs
curl -s localhost:8080/health     # health probe
```

The service is `Restart=always`, enabled at boot, and runs hardened
(`NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, private tmp/devices).

## 5. Upgrade

Re-running the installer *is* the upgrade path — it pulls, rebuilds, republishes
the SPA, and restarts. Your config is left alone.

```bash
sudo /opt/triagekit/scripts/install.sh
```

## 6. Uninstall

```bash
sudo /opt/triagekit/scripts/uninstall.sh           # keep config/data
sudo /opt/triagekit/scripts/uninstall.sh --purge   # remove everything but the DB
```

The database is never touched; drop it yourself if you want the data gone.

---

## Connecting your host app

Once running, point your app at it (see [integration.md](integration.md) for the
full contract). Both use the `TRIAGEKIT_APP_SECRET` from
`/etc/triagekit/triagekit.env`.

```bash
# Provision a user
curl -X POST https://your-host/api/sso/users \
  -H "Authorization: Bearer $TRIAGEKIT_APP_SECRET" \
  -H 'Content-Type: application/json' \
  -d '{"externalUserId":"hms-1","email":"user@example.com","name":"A User"}'

# Mint a login code, then redirect the browser to redirectUrl
curl -X POST https://your-host/api/sso/login-code \
  -H "Authorization: Bearer $TRIAGEKIT_APP_SECRET" \
  -H 'Content-Type: application/json' \
  -d '{"externalUserId":"hms-1","email":"user@example.com","name":"A User","role":"user","returnTo":"/issues"}'
```

## Troubleshooting

**Service won't stay up.** Almost always the database. Check
`journalctl -u triagekit -n 50` and confirm `DB_BACKEND` + the connection string
in `/etc/triagekit/triagekit.env`. The API refuses to start if it can't reach
the database (it connects and seeds on boot).

**Blank page / 404s on assets.** `VITE_APP_BASE_PATH` at build time must match
the nginx location. Re-run `install.sh` after changing it.

**Login works then every request 401s.** `JWT_SECRET` changed, invalidating
issued tokens. Users just need to sign in again.

**API calls hit `localhost:8080` from the browser.** `VITE_API_URL` was set to a
value at build time. Empty it for same-origin and re-run `install.sh`.

**Multiple API instances behind a load balancer.** Set `TOKEN_STORE=redis` so
SSO login codes are shared; the default `memory` store is per-process.

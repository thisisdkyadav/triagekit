#!/usr/bin/env bash
#
# TriageKit installer.
#
#   curl -fsSL https://raw.githubusercontent.com/thisisdkyadav/triagekit/main/scripts/install.sh | sudo bash
#
# or, from a checkout:
#
#   sudo ./scripts/install.sh
#
# Safe to re-run: that is the upgrade path (pull, rebuild, restart). Your
# /etc/triagekit/triagekit.env is never overwritten.
#
set -euo pipefail

# ── Settings (override with env vars) ─────────────────────────
REPO_URL="${TRIAGEKIT_REPO:-https://github.com/thisisdkyadav/triagekit.git}"
BRANCH="${TRIAGEKIT_BRANCH:-main}"
SRC_DIR="${TRIAGEKIT_SRC:-/opt/triagekit}"
CONF_DIR="${TRIAGEKIT_CONF_DIR:-/etc/triagekit}"
CONF_FILE="$CONF_DIR/triagekit.env"
WEB_ROOT="${TRIAGEKIT_WEB_ROOT:-/var/www/triagekit}"
BIN_PATH="${TRIAGEKIT_BIN:-/usr/local/bin/triagekit-server}"
RUN_USER="${TRIAGEKIT_USER:-triagekit}"
SERVICE_NAME=triagekit
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

SKIP_WEB=0
NO_START=0

# ── Pretty output ─────────────────────────────────────────────
if [ -t 1 ]; then
  B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; D=$'\033[2m'; N=$'\033[0m'
else
  B=""; G=""; Y=""; R=""; D=""; N=""
fi
step() { printf '%s==>%s %s%s%s\n' "$G" "$N" "$B" "$*" "$N"; }
info() { printf '    %s\n' "$*"; }
warn() { printf '%s !! %s%s\n' "$Y" "$*" "$N"; }
die()  { printf '%s ✗  %s%s\n' "$R" "$*" "$N" >&2; exit 1; }

usage() {
  cat <<'EOF'
TriageKit installer

Usage: sudo ./scripts/install.sh [options]

Options:
  --skip-web     Only build/install the API (skip the frontend build)
  --no-start     Install everything but don't start the service
  -h, --help     Show this help

Environment overrides:
  TRIAGEKIT_REPO       git URL to clone (default: github.com/thisisdkyadav/triagekit)
  TRIAGEKIT_BRANCH     branch to track (default: main)
  TRIAGEKIT_SRC        source checkout dir (default: /opt/triagekit)
  TRIAGEKIT_CONF_DIR   config dir (default: /etc/triagekit)
  TRIAGEKIT_WEB_ROOT   where the built SPA is copied (default: /var/www/triagekit)
  TRIAGEKIT_BIN        installed binary path (default: /usr/local/bin/triagekit-server)
  TRIAGEKIT_USER       service user (default: triagekit)
  TRIAGEKIT_TARGET_DIR cargo build cache (default: /var/cache/triagekit/target)
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --skip-web) SKIP_WEB=1 ;;
    --no-start) NO_START=1 ;;
    -h|--help)  usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
  shift
done

[ "$(id -u)" -eq 0 ] || die "please run as root (sudo $0)"
command -v systemctl >/dev/null 2>&1 || die "systemd not found; this installer targets systemd Linux"

# Sanity-guard destructive paths.
case "$WEB_ROOT" in ""|"/"|"/usr"|"/etc"|"/var") die "refusing to use WEB_ROOT=$WEB_ROOT" ;; esac

# ── Toolchains ────────────────────────────────────────────────
# rustup/nvm install into the invoking user's home, which root's PATH misses.
adopt_user_path() {
  [ -n "${SUDO_USER:-}" ] || return 0
  local home
  home="$(getent passwd "$SUDO_USER" | cut -d: -f6)" || return 0
  [ -n "$home" ] || return 0
  [ -d "$home/.cargo/bin" ] && export PATH="$home/.cargo/bin:$PATH"
  [ -d "$home/.local/triagekit-node/bin" ] && export PATH="$home/.local/triagekit-node/bin:$PATH"
  return 0
}
adopt_user_path

step "Checking prerequisites"
command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust:
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    then re-run with:  sudo -E env \"PATH=\$PATH\" $0"
info "cargo    $(cargo --version | awk '{print $2}')"
if [ "$SKIP_WEB" -eq 0 ]; then
  command -v npm >/dev/null 2>&1 || die "npm not found. Install Node 18+ (or pass --skip-web)"
  info "npm      $(npm --version)"
fi

# ── Source checkout ───────────────────────────────────────────
# Prefer a local checkout when the script is run from inside one.
LOCAL_REPO=""
SCRIPT_PATH="${BASH_SOURCE[0]:-}"
if [ -n "$SCRIPT_PATH" ] && [ -f "$SCRIPT_PATH" ]; then
  candidate="$(cd "$(dirname "$SCRIPT_PATH")/.." 2>/dev/null && pwd || true)"
  if [ -n "$candidate" ] && [ -f "$candidate/apps/server/Cargo.toml" ]; then
    LOCAL_REPO="$candidate"
  fi
fi

if [ -n "$LOCAL_REPO" ]; then
  SRC_DIR="$LOCAL_REPO"
  step "Using local checkout"
  info "$SRC_DIR"
  info "note: the frontend build writes node_modules/ and dist/ here as root"
else
  command -v git >/dev/null 2>&1 || die "git not found (needed to fetch the source)"
  if [ -d "$SRC_DIR/.git" ]; then
    step "Updating source in $SRC_DIR"
    git -C "$SRC_DIR" fetch --depth 1 origin "$BRANCH"
    git -C "$SRC_DIR" reset --hard "origin/$BRANCH"
  else
    step "Cloning $REPO_URL -> $SRC_DIR"
    mkdir -p "$(dirname "$SRC_DIR")"
    git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$SRC_DIR"
  fi
fi

[ -f "$SRC_DIR/apps/server/Cargo.toml" ] || die "$SRC_DIR does not look like a TriageKit checkout"

# ── Service user ──────────────────────────────────────────────
step "Ensuring service user '$RUN_USER'"
if id -u "$RUN_USER" >/dev/null 2>&1; then
  info "already exists"
else
  nologin="$(command -v nologin || echo /bin/false)"
  useradd --system --no-create-home --shell "$nologin" "$RUN_USER"
  info "created"
fi

# ── Config ────────────────────────────────────────────────────
# Note: no `| head -c N` here — under `set -o pipefail` head closes the pipe
# early, the upstream process takes SIGPIPE, and the whole script would abort.
# Generate first, then truncate with bash substring expansion.
rand_hex() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n'
  fi
}
rand_pw() {
  local raw
  raw="$(rand_hex)"
  printf '%s' "${raw:0:20}"
}
set_kv() { # set_kv KEY VALUE FILE — only ever called with generated
           # hex/alphanumeric values, so they are safe as a sed replacement.
  local key="$1" val="$2" file="$3"
  if grep -q "^${key}=" "$file"; then
    sed -i "s|^${key}=.*|${key}=${val}|" "$file"
  else
    printf '%s=%s\n' "$key" "$val" >> "$file"
  fi
}

FRESH_CONFIG=0
ADMIN_PW=""
step "Configuring $CONF_FILE"
mkdir -p "$CONF_DIR"
if [ -f "$CONF_FILE" ]; then
  info "existing config kept (edit it, then re-run this script)"
else
  install -m 0640 "$SRC_DIR/deploy/triagekit.env.example" "$CONF_FILE"
  ADMIN_PW="$(rand_pw)"
  set_kv TRIAGEKIT_APP_SECRET   "$(rand_hex)" "$CONF_FILE"
  set_kv JWT_SECRET             "$(rand_hex)" "$CONF_FILE"
  set_kv BOOTSTRAP_ADMIN_PASSWORD "$ADMIN_PW"  "$CONF_FILE"
  FRESH_CONFIG=1
  info "created with generated secrets"
fi
chown root:"$RUN_USER" "$CONF_FILE"
chmod 0640 "$CONF_FILE"

# Load config so the frontend build gets VITE_* and we know the port.
set -a
# shellcheck disable=SC1090
. "$CONF_FILE"
set +a

# ── Build API ─────────────────────────────────────────────────
# Build into a shared cache rather than the checkout, so running this from a
# user-owned clone doesn't litter it with root-owned target/ artifacts.
export CARGO_TARGET_DIR="${TRIAGEKIT_TARGET_DIR:-/var/cache/triagekit/target}"
mkdir -p "$CARGO_TARGET_DIR"

step "Building the API (release)"
info "this takes a few minutes on first run"
cargo build --release --manifest-path "$SRC_DIR/apps/server/Cargo.toml"
install -m 0755 "$CARGO_TARGET_DIR/release/triagekit-server" "$BIN_PATH"
info "installed $BIN_PATH"

# ── Build web ─────────────────────────────────────────────────
if [ "$SKIP_WEB" -eq 0 ]; then
  step "Building the web app"
  info "VITE_APP_BASE_PATH=${VITE_APP_BASE_PATH:-/issue-management}"
  info "VITE_API_URL=${VITE_API_URL:-(same origin)}"
  if [ -f "$SRC_DIR/apps/web/package-lock.json" ]; then
    npm --prefix "$SRC_DIR/apps/web" ci --no-audit --no-fund
  else
    npm --prefix "$SRC_DIR/apps/web" install --no-audit --no-fund
  fi
  npm --prefix "$SRC_DIR/apps/web" run build

  step "Publishing the SPA to $WEB_ROOT"
  mkdir -p "$WEB_ROOT"
  find "$WEB_ROOT" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
  cp -r "$SRC_DIR/apps/web/dist/." "$WEB_ROOT/"
  chmod -R a+rX "$WEB_ROOT"
  info "$(find "$WEB_ROOT" -type f | wc -l) files"
else
  warn "skipping frontend build (--skip-web)"
fi

# ── systemd ───────────────────────────────────────────────────
step "Installing systemd unit"
sed -e "s|^User=.*|User=${RUN_USER}|" \
    -e "s|^Group=.*|Group=${RUN_USER}|" \
    -e "s|^EnvironmentFile=.*|EnvironmentFile=${CONF_FILE}|" \
    -e "s|^ExecStart=.*|ExecStart=${BIN_PATH}|" \
    "$SRC_DIR/deploy/triagekit.service" > "$SERVICE_FILE"
chmod 0644 "$SERVICE_FILE"
systemctl daemon-reload
systemctl enable "$SERVICE_NAME" >/dev/null 2>&1 || true
info "$SERVICE_FILE"

if [ "$NO_START" -eq 1 ]; then
  warn "not starting (--no-start). Start with: systemctl start $SERVICE_NAME"
else
  step "Starting $SERVICE_NAME"
  systemctl restart "$SERVICE_NAME"

  port="${SERVER_PORT:-8080}"
  host="${SERVER_HOST:-127.0.0.1}"
  ok=0
  for _ in $(seq 1 20); do
    if curl -fsS "http://${host}:${port}/health" >/dev/null 2>&1; then ok=1; break; fi
    sleep 1
  done
  if [ "$ok" -eq 1 ]; then
    printf '    %s✓ API healthy on http://%s:%s%s\n' "$G" "$host" "$port" "$N"
  else
    warn "service did not answer /health yet. Check:"
    info "  systemctl status $SERVICE_NAME"
    info "  journalctl -u $SERVICE_NAME -n 50 --no-pager"
    info "Most common cause: the database in $CONF_FILE isn't running/reachable."
  fi
fi

# ── Summary ───────────────────────────────────────────────────
BASE_PATH_OUT="${APP_BASE_PATH:-/issue-management}"
cat <<EOF

${B}TriageKit is installed.${N}

  Config      ${CONF_FILE}
  Binary      ${BIN_PATH}
  Web root    ${WEB_ROOT}
  Service     systemctl status ${SERVICE_NAME}
  Logs        journalctl -u ${SERVICE_NAME} -f

${B}Next: nginx${N}
  Copy the snippet from ${SRC_DIR}/deploy/nginx.conf.example into your site's
  server block (it serves the SPA at ${BASE_PATH_OUT} and proxies /api), then:

      nginx -t && systemctl reload nginx

${B}Upgrading later${N}
      sudo ${SRC_DIR}/scripts/install.sh
EOF

if [ "$FRESH_CONFIG" -eq 1 ]; then
  cat <<EOF

${Y}${B}Save these now — shown only once:${N}
  Owner login   ${BOOTSTRAP_ADMIN_EMAIL:-admin@example.com}
  Password      ${ADMIN_PW}
  App secret    (host apps authenticate with this) see TRIAGEKIT_APP_SECRET in ${CONF_FILE}

${Y}Edit ${CONF_FILE} to set PUBLIC_WEB_URL and your database, then re-run this script.${N}
EOF
fi

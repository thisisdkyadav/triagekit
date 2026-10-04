#!/usr/bin/env bash
#
# TriageKit installer.
#
#   curl -fsSL https://raw.githubusercontent.com/thisisdkyadav/triagekit/main/scripts/install.sh | sudo bash
#
# By default this DOWNLOADS a prebuilt static binary and web bundle from the
# latest GitHub release, so the server needs no Rust or Node toolchain — just
# set env in one file and run. Pass --from-source to build locally instead.
#
# Safe to re-run: that is the upgrade path. Your /etc/triagekit/triagekit.env
# is never overwritten.
#
set -euo pipefail

# ── Settings (override with env vars) ─────────────────────────
REPO_URL="${TRIAGEKIT_REPO:-https://github.com/thisisdkyadav/triagekit.git}"
RELEASE_REPO="${TRIAGEKIT_RELEASE_REPO:-thisisdkyadav/triagekit}"
BRANCH="${TRIAGEKIT_BRANCH:-main}"
VERSION="${TRIAGEKIT_VERSION:-latest}"
SRC_DIR="${TRIAGEKIT_SRC:-}"
CONF_DIR="${TRIAGEKIT_CONF_DIR:-/etc/triagekit}"
CONF_FILE="$CONF_DIR/triagekit.env"
WEB_ROOT="${TRIAGEKIT_WEB_ROOT:-/var/www/triagekit}"
BIN_PATH="${TRIAGEKIT_BIN:-/usr/local/bin/triagekit-server}"
ASSET_DIR="${TRIAGEKIT_ASSET_DIR:-/usr/local/share/triagekit}"
RUN_USER="${TRIAGEKIT_USER:-triagekit}"
SERVICE_NAME=triagekit
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

MODE=auto          # auto | prebuilt | source
SKIP_WEB=0
NO_START=0

# Web bundle in a release is built with these; the installer can only reuse the
# prebuilt SPA when the target config matches them (else it builds from source).
WEB_DEFAULT_BASE_PATH=/issue-management

# Release download / raw-file bases (resolved after arg parsing).
if [ "$VERSION" = "latest" ]; then
  DL_BASE="https://github.com/${RELEASE_REPO}/releases/latest/download"
  RAW_REF="$BRANCH"
else
  DL_BASE="https://github.com/${RELEASE_REPO}/releases/download/${VERSION}"
  RAW_REF="$VERSION"
fi
RAW_BASE="https://raw.githubusercontent.com/${RELEASE_REPO}/${RAW_REF}"

TMP=""
cleanup() { [ -n "$TMP" ] && rm -rf "$TMP" || true; }
trap cleanup EXIT

# ── Pretty output ─────────────────────────────────────────────
if [ -t 1 ]; then
  B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; N=$'\033[0m'
else
  B=""; G=""; Y=""; R=""; N=""
fi
step() { printf '%s==>%s %s%s%s\n' "$G" "$N" "$B" "$*" "$N"; }
info() { printf '    %s\n' "$*"; }
warn() { printf '%s !! %s%s\n' "$Y" "$*" "$N"; }
die()  { printf '%s x  %s%s\n' "$R" "$*" "$N" >&2; exit 1; }

usage() {
  cat <<'EOF'
TriageKit installer

Usage: sudo ./scripts/install.sh [options]

Options:
  --from-source   Build the binary (and web) locally instead of downloading
                  a release. Requires Rust (and Node unless --skip-web).
  --prebuilt      Force downloading the prebuilt release (default unless run
                  from inside a source checkout).
  --version TAG   Install a specific release tag (e.g. v0.1.0). Default: latest.
  --skip-web      Only install the API (you serve the SPA yourself).
  --no-start      Install everything but don't start the service.
  -h, --help      Show this help.

Environment overrides:
  TRIAGEKIT_VERSION       release tag to install (default: latest)
  TRIAGEKIT_RELEASE_REPO  owner/repo for releases (default: thisisdkyadav/triagekit)
  TRIAGEKIT_REPO          git URL for --from-source (default: same repo)
  TRIAGEKIT_BRANCH        branch for source/templates (default: main)
  TRIAGEKIT_SRC           source checkout dir (default: /opt/triagekit)
  TRIAGEKIT_CONF_DIR      config dir (default: /etc/triagekit)
  TRIAGEKIT_WEB_ROOT      where the SPA is published (default: /var/www/triagekit)
  TRIAGEKIT_BIN           installed binary path (default: /usr/local/bin/triagekit-server)
  TRIAGEKIT_USER          service user (default: triagekit)
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --from-source) MODE=source ;;
    --prebuilt)    MODE=prebuilt ;;
    --version)     shift; [ $# -gt 0 ] || die "--version needs a tag"; VERSION="$1"
                   DL_BASE="https://github.com/${RELEASE_REPO}/releases/download/${VERSION}"
                   RAW_REF="$VERSION"
                   RAW_BASE="https://raw.githubusercontent.com/${RELEASE_REPO}/${RAW_REF}" ;;
    --skip-web)    SKIP_WEB=1 ;;
    --no-start)    NO_START=1 ;;
    -h|--help)     usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
  shift
done

[ "$(id -u)" -eq 0 ] || die "please run as root (sudo $0)"
command -v systemctl >/dev/null 2>&1 || die "systemd not found; this installer targets systemd Linux"
command -v curl >/dev/null 2>&1 || die "curl is required"

case "$WEB_ROOT" in ""|"/"|"/usr"|"/etc"|"/var") die "refusing to use WEB_ROOT=$WEB_ROOT" ;; esac

TMP="$(mktemp -d)"

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) PREBUILT_ASSET="triagekit-server-linux-x86_64" ;;
  *)            PREBUILT_ASSET="" ;;
esac

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

# ── Detect a local checkout & resolve mode ────────────────────
LOCAL_REPO=""
SCRIPT_PATH="${BASH_SOURCE[0]:-}"
if [ -n "$SCRIPT_PATH" ] && [ -f "$SCRIPT_PATH" ]; then
  candidate="$(cd "$(dirname "$SCRIPT_PATH")/.." 2>/dev/null && pwd || true)"
  if [ -n "$candidate" ] && [ -f "$candidate/apps/server/Cargo.toml" ]; then
    LOCAL_REPO="$candidate"
    SRC_DIR="$candidate"
  fi
fi

if [ "$MODE" = auto ]; then
  # From a checkout, build it; piped from the internet, download a release.
  if [ -n "$LOCAL_REPO" ]; then MODE=source; else MODE=prebuilt; fi
fi

step "Install mode: ${B}${MODE}${N}${G}  (version: ${VERSION}, arch: ${ARCH})"
[ -n "$LOCAL_REPO" ] && info "local checkout: $LOCAL_REPO"

# ── Helpers ───────────────────────────────────────────────────
# Give back a local path to a repo file (deploy/…): from the checkout if we
# have one, otherwise fetched from raw.githubusercontent.
fetch_repo_file() {
  local rel="$1"
  if [ -n "${SRC_DIR:-}" ] && [ -f "$SRC_DIR/$rel" ]; then
    printf '%s' "$SRC_DIR/$rel"; return 0
  fi
  local dest="$TMP/$(basename "$rel")"
  curl -fsSL -o "$dest" "$RAW_BASE/$rel" || return 1
  printf '%s' "$dest"
}

SUMS_FILE=""
ensure_sums() {
  [ -z "$SUMS_FILE" ] || return 0
  local f="$TMP/SHA256SUMS"
  curl -fSL --retry 3 -o "$f" "$DL_BASE/SHA256SUMS" || return 1
  SUMS_FILE="$f"
}

# download_verified ASSET DEST  → 0 on success (checksum-checked)
download_verified() {
  local name="$1" dest="$2"
  ensure_sums || { warn "could not fetch SHA256SUMS from the release"; return 1; }
  curl -fSL --retry 3 -o "$dest" "$DL_BASE/$name" || { warn "download failed: $name"; return 1; }
  local want have
  want="$(awk -v n="$name" '$2==n || $2=="*"n {print $1}' "$SUMS_FILE" | head -1)"
  [ -n "$want" ] || { warn "no checksum listed for $name"; return 1; }
  have="$(sha256sum "$dest" | awk '{print $1}')"
  [ "$want" = "$have" ] || { warn "checksum mismatch for $name"; return 1; }
  return 0
}

ensure_full_checkout() {
  if [ -n "${SRC_DIR:-}" ] && [ -f "$SRC_DIR/apps/server/Cargo.toml" ]; then return 0; fi
  command -v git >/dev/null 2>&1 || die "git is required to fetch the source"
  SRC_DIR="${SRC_DIR:-/opt/triagekit}"
  if [ -d "$SRC_DIR/.git" ]; then
    step "Updating source in $SRC_DIR"
    git -C "$SRC_DIR" fetch --depth 1 origin "$RAW_REF"
    git -C "$SRC_DIR" reset --hard FETCH_HEAD
  else
    step "Cloning $REPO_URL -> $SRC_DIR"
    mkdir -p "$(dirname "$SRC_DIR")"
    git clone --depth 1 --branch "$RAW_REF" "$REPO_URL" "$SRC_DIR"
  fi
  [ -f "$SRC_DIR/apps/server/Cargo.toml" ] || die "$SRC_DIR is not a TriageKit checkout"
}

# ── Prereqs for the chosen mode ───────────────────────────────
step "Checking prerequisites"
if [ "$MODE" = source ]; then
  command -v cargo >/dev/null 2>&1 || die "cargo not found (needed for --from-source). Install Rust:
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    then re-run:  sudo -E env \"PATH=\$PATH\" $0 --from-source"
  info "cargo    $(cargo --version | awk '{print $2}')"
else
  [ -n "$PREBUILT_ASSET" ] || die "no prebuilt binary for arch '$ARCH'. Re-run with --from-source."
  command -v sha256sum >/dev/null 2>&1 || die "sha256sum is required for verified downloads"
  command -v tar >/dev/null 2>&1 || die "tar is required"
  info "will download release '$VERSION' for $ARCH"
fi

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
rand_hex() {
  if command -v openssl >/dev/null 2>&1; then openssl rand -hex 32
  else head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n'; fi
}
rand_pw() { local raw; raw="$(rand_hex)"; printf '%s' "${raw:0:20}"; }
set_kv() { # values are generated hex/alnum, safe as a sed replacement
  local key="$1" val="$2" file="$3"
  if grep -q "^${key}=" "$file"; then sed -i "s|^${key}=.*|${key}=${val}|" "$file"
  else printf '%s=%s\n' "$key" "$val" >> "$file"; fi
}

FRESH_CONFIG=0
ADMIN_PW=""
step "Configuring $CONF_FILE"
mkdir -p "$CONF_DIR"
if [ -f "$CONF_FILE" ]; then
  info "existing config kept (edit it, then re-run this script)"
else
  env_example="$(fetch_repo_file deploy/triagekit.env.example)" \
    || die "could not obtain deploy/triagekit.env.example"
  install -m 0640 "$env_example" "$CONF_FILE"
  ADMIN_PW="$(rand_pw)"
  set_kv TRIAGEKIT_APP_SECRET     "$(rand_hex)" "$CONF_FILE"
  set_kv JWT_SECRET               "$(rand_hex)" "$CONF_FILE"
  set_kv BOOTSTRAP_ADMIN_PASSWORD "$ADMIN_PW"   "$CONF_FILE"
  FRESH_CONFIG=1
  info "created with generated secrets"
fi
chown root:"$RUN_USER" "$CONF_FILE"
chmod 0640 "$CONF_FILE"

# Load config: we need the port for the health check and VITE_* for the web step.
set -a
# shellcheck disable=SC1090
. "$CONF_FILE"
set +a

# ── API binary ────────────────────────────────────────────────
if [ "$MODE" = source ]; then
  ensure_full_checkout
  export CARGO_TARGET_DIR="${TRIAGEKIT_TARGET_DIR:-/var/cache/triagekit/target}"
  mkdir -p "$CARGO_TARGET_DIR"
  step "Building the API from source (release)"
  info "this takes a few minutes on first run"
  cargo build --release --locked --manifest-path "$SRC_DIR/apps/server/Cargo.toml"
  install -m 0755 "$CARGO_TARGET_DIR/release/triagekit-server" "$BIN_PATH"
else
  step "Downloading prebuilt API binary ($VERSION)"
  if download_verified "$PREBUILT_ASSET" "$TMP/triagekit-server"; then
    install -m 0755 "$TMP/triagekit-server" "$BIN_PATH"
    info "installed $BIN_PATH (checksum verified)"
  elif command -v cargo >/dev/null 2>&1; then
    warn "falling back to building from source"
    MODE=source
    ensure_full_checkout
    export CARGO_TARGET_DIR="${TRIAGEKIT_TARGET_DIR:-/var/cache/triagekit/target}"
    mkdir -p "$CARGO_TARGET_DIR"
    cargo build --release --locked --manifest-path "$SRC_DIR/apps/server/Cargo.toml"
    install -m 0755 "$CARGO_TARGET_DIR/release/triagekit-server" "$BIN_PATH"
  else
    die "could not download the release binary and cargo isn't available to build.
    Has a release been published at github.com/${RELEASE_REPO}/releases ?
    Otherwise install Rust and re-run with --from-source."
  fi
fi
info "binary: $BIN_PATH"

# ── Web app ───────────────────────────────────────────────────
web_defaults_match() {
  [ "${VITE_APP_BASE_PATH:-$WEB_DEFAULT_BASE_PATH}" = "$WEB_DEFAULT_BASE_PATH" ] \
    && [ -z "${VITE_API_URL:-}" ]
}
publish_web_dir() { # publish_web_dir SRC_DIR_OF_FILES
  step "Publishing the SPA to $WEB_ROOT"
  mkdir -p "$WEB_ROOT"
  find "$WEB_ROOT" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
  cp -r "$1/." "$WEB_ROOT/"
  chmod -R a+rX "$WEB_ROOT"
  info "$(find "$WEB_ROOT" -type f | wc -l) files"
}
build_web_from_source() {
  command -v npm >/dev/null 2>&1 || die "building the web app needs Node/npm.
    Options: install Node 18+, OR keep APP_BASE_PATH/VITE_APP_BASE_PATH at
    '$WEB_DEFAULT_BASE_PATH' with an empty VITE_API_URL to use the prebuilt
    bundle, OR pass --skip-web and serve the SPA yourself."
  ensure_full_checkout
  step "Building the web app from source"
  info "VITE_APP_BASE_PATH=${VITE_APP_BASE_PATH:-$WEB_DEFAULT_BASE_PATH}  VITE_API_URL=${VITE_API_URL:-(same origin)}"
  if [ -f "$SRC_DIR/apps/web/package-lock.json" ]; then
    npm --prefix "$SRC_DIR/apps/web" ci --no-audit --no-fund
  else
    npm --prefix "$SRC_DIR/apps/web" install --no-audit --no-fund
  fi
  npm --prefix "$SRC_DIR/apps/web" run build
  publish_web_dir "$SRC_DIR/apps/web/dist"
}

if [ "$SKIP_WEB" -eq 1 ]; then
  warn "skipping frontend (--skip-web)"
elif [ "$MODE" != source ] && web_defaults_match; then
  step "Downloading prebuilt web bundle ($VERSION)"
  if download_verified triagekit-web.tar.gz "$TMP/web.tar.gz"; then
    mkdir -p "$TMP/web" && tar -xzf "$TMP/web.tar.gz" -C "$TMP/web"
    publish_web_dir "$TMP/web"
    info "checksum verified"
  else
    warn "prebuilt web download failed; building from source"
    build_web_from_source
  fi
else
  [ "$MODE" = source ] || info "custom base path / API URL -> building web from source"
  build_web_from_source
fi

# ── nginx template (kept on disk for reference) ───────────────
if nginx_tpl="$(fetch_repo_file deploy/nginx.conf.example 2>/dev/null)"; then
  mkdir -p "$ASSET_DIR"
  install -m 0644 "$nginx_tpl" "$ASSET_DIR/nginx.conf.example"
  NGINX_REF="$ASSET_DIR/nginx.conf.example"
else
  NGINX_REF="${SRC_DIR:-<repo>}/deploy/nginx.conf.example"
fi

# ── systemd ───────────────────────────────────────────────────
step "Installing systemd unit"
service_tpl="$(fetch_repo_file deploy/triagekit.service)" \
  || die "could not obtain deploy/triagekit.service"
sed -e "s|^User=.*|User=${RUN_USER}|" \
    -e "s|^Group=.*|Group=${RUN_USER}|" \
    -e "s|^EnvironmentFile=.*|EnvironmentFile=${CONF_FILE}|" \
    -e "s|^ExecStart=.*|ExecStart=${BIN_PATH}|" \
    "$service_tpl" > "$SERVICE_FILE"
chmod 0644 "$SERVICE_FILE"
systemctl daemon-reload
systemctl enable "$SERVICE_NAME" >/dev/null 2>&1 || true
info "$SERVICE_FILE"

if [ "$NO_START" -eq 1 ]; then
  warn "not starting (--no-start). Start with: systemctl start $SERVICE_NAME"
else
  step "Starting $SERVICE_NAME"
  systemctl restart "$SERVICE_NAME"
  port="${SERVER_PORT:-8080}"; host="${SERVER_HOST:-127.0.0.1}"
  ok=0
  for _ in $(seq 1 20); do
    if curl -fsS "http://${host}:${port}/health" >/dev/null 2>&1; then ok=1; break; fi
    sleep 1
  done
  if [ "$ok" -eq 1 ]; then
    printf '    %s* API healthy on http://%s:%s%s\n' "$G" "$host" "$port" "$N"
  else
    warn "service did not answer /health yet. Check:"
    info "  systemctl status $SERVICE_NAME"
    info "  journalctl -u $SERVICE_NAME -n 50 --no-pager"
    info "Most common cause: the database in $CONF_FILE isn't running/reachable."
  fi
fi

# ── Summary ───────────────────────────────────────────────────
BASE_PATH_OUT="${APP_BASE_PATH:-$WEB_DEFAULT_BASE_PATH}"
if [ -n "${SRC_DIR:-}" ] && [ -f "${SRC_DIR}/scripts/install.sh" ]; then
  UPGRADE_CMD="sudo ${SRC_DIR}/scripts/install.sh"
else
  UPGRADE_CMD="curl -fsSL https://raw.githubusercontent.com/${RELEASE_REPO}/${BRANCH}/scripts/install.sh | sudo bash"
fi

cat <<EOF

${B}TriageKit is installed.${N}

  Config      ${CONF_FILE}
  Binary      ${BIN_PATH}
  Web root    ${WEB_ROOT}
  Service     systemctl status ${SERVICE_NAME}
  Logs        journalctl -u ${SERVICE_NAME} -f

${B}Next: nginx${N}
  Copy the snippet from ${NGINX_REF} into your site's server block
  (serves the SPA at ${BASE_PATH_OUT} and proxies /api), then:

      nginx -t && systemctl reload nginx

${B}Upgrading later${N}
      ${UPGRADE_CMD}
EOF

if [ "$FRESH_CONFIG" -eq 1 ]; then
  cat <<EOF

${Y}${B}Save these now - shown only once:${N}
  Owner login   ${BOOTSTRAP_ADMIN_EMAIL:-admin@example.com}
  Password      ${ADMIN_PW}
  App secret    see TRIAGEKIT_APP_SECRET in ${CONF_FILE}

${Y}Edit ${CONF_FILE} to set PUBLIC_WEB_URL and your database, then re-run.${N}
EOF
fi

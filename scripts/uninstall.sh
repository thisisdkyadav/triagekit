#!/usr/bin/env bash
#
# Remove a TriageKit installation.
#
#   sudo ./scripts/uninstall.sh            # keep config + data
#   sudo ./scripts/uninstall.sh --purge    # also remove config, web root, source, user
#
# Your database is NEVER touched — drop it yourself if you want the data gone.
#
set -euo pipefail

CONF_DIR="${TRIAGEKIT_CONF_DIR:-/etc/triagekit}"
WEB_ROOT="${TRIAGEKIT_WEB_ROOT:-/var/www/triagekit}"
BIN_PATH="${TRIAGEKIT_BIN:-/usr/local/bin/triagekit-server}"
SRC_DIR="${TRIAGEKIT_SRC:-/opt/triagekit}"
RUN_USER="${TRIAGEKIT_USER:-triagekit}"
SERVICE_NAME=triagekit
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

PURGE=0
[ "${1:-}" = "--purge" ] && PURGE=1

[ "$(id -u)" -eq 0 ] || { echo "please run as root (sudo $0)" >&2; exit 1; }

case "$WEB_ROOT" in ""|"/"|"/usr"|"/etc"|"/var") echo "refusing WEB_ROOT=$WEB_ROOT" >&2; exit 1 ;; esac
case "$SRC_DIR"  in ""|"/"|"/usr"|"/etc"|"/var") echo "refusing SRC_DIR=$SRC_DIR"  >&2; exit 1 ;; esac

echo "==> Stopping $SERVICE_NAME"
systemctl disable --now "$SERVICE_NAME" 2>/dev/null || true
rm -f "$SERVICE_FILE"
systemctl daemon-reload
systemctl reset-failed "$SERVICE_NAME" 2>/dev/null || true

echo "==> Removing binary"
rm -f "$BIN_PATH"

if [ "$PURGE" -eq 1 ]; then
  echo "==> Purging config, web root, source, and user"
  rm -rf "$CONF_DIR" "$WEB_ROOT" "$SRC_DIR" /var/lib/triagekit
  if id -u "$RUN_USER" >/dev/null 2>&1; then
    userdel "$RUN_USER" 2>/dev/null || true
  fi
  echo "    done (database untouched)"
else
  echo "==> Kept: $CONF_DIR, $WEB_ROOT, $SRC_DIR"
  echo "    Re-run with --purge to remove them too."
fi

echo
echo "Remember to remove the TriageKit block from your nginx config and reload."

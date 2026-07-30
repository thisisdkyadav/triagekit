#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCAL_NODE_BIN="$HOME/.local/triagekit-node/bin"

if [ -d "$LOCAL_NODE_BIN" ]; then
  export PATH="$LOCAL_NODE_BIN:$PATH"
fi

export TMPDIR="${TMPDIR:-/tmp}"

cleanup() {
  jobs -p | xargs -r kill
}

trap cleanup EXIT INT TERM

cd "$ROOT_DIR"

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

echo "Cleaning old TriageKit processes..."
pkill -f "$ROOT_DIR/apps/web/node_modules/.bin/vite" 2>/dev/null || true
pkill -f "$ROOT_DIR/apps/server/target/debug/triagekit-server" 2>/dev/null || true
pkill -f "target/debug/triagekit-server" 2>/dev/null || true
for port in 8080; do
  pid="$(lsof -ti tcp:$port 2>/dev/null || true)"
  if [ -n "$pid" ]; then
    kill $pid 2>/dev/null || true
  fi
done
sleep 1

echo "Starting Rust API (DB_BACKEND=${DB_BACKEND:-mongo})..."
echo "  Ensure the selected database is running:"
echo "    mongo    -> mongod at ${MONGO_URI:-mongodb://127.0.0.1:27017}"
echo "    postgres -> ${DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:5432/triagekit}"
(cd apps/server && cargo run) &

echo "Starting Vite..."
npm run dev:web &

wait

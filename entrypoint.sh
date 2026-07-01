#!/bin/bash
set -e

mkdir -p /var/lib/tailscale /var/run/tailscale

./bin/tailscaled --state=/var/lib/tailscale/tailscaled.state --socket=/var/run/tailscale/tailscaled.sock &
TAILSCALED_PID=$!

for i in $(seq 1 30); do
  if ./bin/tailscale --socket=/var/run/tailscale/tailscaled.sock status >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if [ -n "$TS_AUTHKEY" ]; then
  ./bin/tailscale --socket=/var/run/tailscale/tailscaled.sock up --authkey="$TS_AUTHKEY" --hostname="${TS_HOSTNAME:-tass-web}" ${TS_EXTRA_ARGS:-}
fi

exec ./bin/tass-web

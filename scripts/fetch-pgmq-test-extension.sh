#!/usr/bin/env bash
# Fetch actual upstream SQL/control files for an isolated test server, never production.
set -euo pipefail
if [[ $# != 1 ]]; then
  echo 'usage: fetch-pgmq-test-extension.sh <destination-extension-directory>' >&2
  exit 2
fi
mkdir -p "$1"
pgmq_commit=12462bd29c2e4f59e58ac4840c2dcf0872d8c5c9
pgmq_source="https://raw.githubusercontent.com/pgmq/pgmq/${pgmq_commit}/pgmq-extension"
curl --fail --silent --show-error --location "${pgmq_source}/pgmq.control" -o "$1/pgmq.control"
curl --fail --silent --show-error --location "${pgmq_source}/sql/pgmq.sql" -o "$1/pgmq--1.8.1.sql"
(
  cd "$1"
  sha256sum --check <<'CHECKSUMS'
41c1e721b7b2d22484ee40842f2bbc52f571c36ac8bb50eeb3af5e724bdd43e5  pgmq.control
c2679044f87f4ac9693ef4af61786e23dafe90fb1b3bfa0228286a495cbd7a97  pgmq--1.8.1.sql
CHECKSUMS
)

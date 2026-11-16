#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 2 ]]; then
  echo 'usage: smoke-container.sh IMAGE FIXTURE_JSON' >&2
  exit 2
fi
image="$1"
fixture="$2"
smoke_dir="$(mktemp -d)"
worker_name="labyrinth-worker-smoke-$$"
api_name="labyrinth-api-smoke-$$"
cleanup() {
  docker rm -f "$worker_name" "$api_name" >/dev/null 2>&1 || true
  rm -rf "$smoke_dir"
}
trap cleanup EXIT
[[ "$(docker image inspect "$image" --format '{{.Config.User}}')" == '65532:65532' ]]
timeout 150s docker run --rm --name "$worker_name" --network none --read-only \
  --memory 2g --cpus 1 --pids-limit 64 --cap-drop ALL --security-opt no-new-privileges \
  --entrypoint /usr/bin/prlimit -i "$image" --cpu=60:60 --as=1073741824:1073741824 \
  --core=0:0 -- /app/labyrinth-worker --engine-child < "$fixture" | head -c 134217729 > "$smoke_dir/package-wire.json"
python3 scripts/verify-container-package.py "$fixture" "$smoke_dir/package-wire.json"
if timeout 10s docker run --rm --name "$api_name" --network none --read-only \
  --memory 512m --pids-limit 64 --cap-drop ALL --security-opt no-new-privileges \
  "$image" > "$smoke_dir/api.log" 2>&1; then
  echo 'API unexpectedly started without required configuration' >&2
  exit 1
fi
# A timeout would also be nonzero: require explicit missing-config failure evidence.
if ! grep -Eq 'LABYRINTH_BIND|invalid or missing server configuration' "$smoke_dir/api.log"; then
  echo 'API did not report a bounded missing-configuration startup failure' >&2
  exit 1
fi
echo 'Container smoke checks passed without network or hosted credentials'

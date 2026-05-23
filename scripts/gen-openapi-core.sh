#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SPEC="${REPO_ROOT}/src/runtime/openapi/ws-core.yaml"
OUT_DIR="${REPO_ROOT}/src/transport/src/ws/api_gen"

mkdir -p "${OUT_DIR}"

strip_rustfmt_skip() {
  python3 - "${OUT_DIR}" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
prefixes = ("#![rustfmt::skip]\n\n", "#![rustfmt::skip]\r\n\r\n")

for path in root.rglob("*.rs"):
    text = path.read_text()
    for prefix in prefixes:
        if text.startswith(prefix):
            path.write_text(text[len(prefix):])
            break
PY
}

run_with_docker() {
  if command -v docker >/dev/null 2>&1; then
    docker run --rm -v "${REPO_ROOT}:/local" openapitools/openapi-generator-cli:v7.9.0 \
      generate -i /local/src/runtime/openapi/ws-core.yaml -g rust -o /local/src/transport/src/ws/api_gen --global-property models \
      && return 0
  fi
  return 1
}

run_with_local_jar() {
  local JAR_PATH="${OPENAPI_GEN_JAR:-${REPO_ROOT}/openapi-generator-cli.jar}"
  if [ -f "${JAR_PATH}" ]; then
    java -jar "${JAR_PATH}" generate -i "${SPEC}" -g rust -o "${OUT_DIR}" --global-property models
    return 0
  fi
  return 1
}

if run_with_docker; then
  strip_rustfmt_skip
  exit 0
elif run_with_local_jar; then
  strip_rustfmt_skip
  exit 0
else
  echo "ERROR: Could not run OpenAPI generation (Docker unavailable and no local jar)." >&2
  echo "Install Docker or download openapi-generator-cli-7.9.0.jar to openapi-generator-cli.jar" >&2
  exit 1
fi

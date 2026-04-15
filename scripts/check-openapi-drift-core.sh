#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

TARGET_PATHS=("src/transport/src/ws/api_gen")
before="$(git status --porcelain -- "${TARGET_PATHS[@]}" | LC_ALL=C sort)"

bash "${REPO_ROOT}/scripts/gen-openapi-core.sh"

after="$(git status --porcelain -- "${TARGET_PATHS[@]}" | LC_ALL=C sort)"

if [[ "${before}" != "${after}" ]]; then
  echo "ERROR: Core OpenAPI generated artifacts are out of date."
  echo "Run: bash scripts/gen-openapi-core.sh"
  git diff -- "${TARGET_PATHS[@]}"
  exit 1
fi

echo "Core OpenAPI artifacts are up to date."

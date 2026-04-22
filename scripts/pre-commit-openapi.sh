#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

STAGED_FILES="$(git diff --cached --name-only --diff-filter=ACMR)"

if [[ -z "${STAGED_FILES}" ]]; then
  exit 0
fi

staged_matches() {
  local pattern
  while IFS= read -r pattern; do
    [[ -z "${pattern}" ]] && continue
    while IFS= read -r staged; do
      case "${staged}" in
        ${pattern})
          return 0
          ;;
      esac
    done <<<"${STAGED_FILES}"
  done
  return 1
}

ensure_core_generator_available() {
  local jar_path="${OPENAPI_GEN_JAR:-${REPO_ROOT}/openapi-generator-cli.jar}"
  if command -v docker >/dev/null 2>&1 || [[ -f "${jar_path}" ]]; then
    return 0
  fi

  echo "ERROR: Core OpenAPI generation is required for this commit." >&2
  echo "Install Docker or provide a generator jar at ${jar_path}." >&2
  exit 1
}

needs_core_codegen=false
needs_suite_check=false

if staged_matches <<'EOF'
src/runtime/openapi/ws-core.yaml
scripts/gen-openapi-core.sh
scripts/check-openapi-drift-core.sh
src/transport/src/ws/api_gen/*
EOF
then
  needs_core_codegen=true
fi

if staged_matches <<'EOF'
suites/data_engineer/openapi/ws-data-engineer.yaml
scripts/gen-openapi-suite-data-engineer.sh
scripts/check-openapi-drift-suite-data-engineer.sh
EOF
then
  needs_suite_check=true
fi

if [[ "${needs_core_codegen}" == "false" && "${needs_suite_check}" == "false" ]]; then
  exit 0
fi

if [[ "${needs_core_codegen}" == "true" ]]; then
  ensure_core_generator_available
  echo "Running core OpenAPI generation..."
  bash "${REPO_ROOT}/scripts/gen-openapi-core.sh"
  git add -- "src/transport/src/ws/api_gen"
  bash "${REPO_ROOT}/scripts/check-openapi-drift-core.sh" >/dev/null
fi

if [[ "${needs_suite_check}" == "true" ]]; then
  bash "${REPO_ROOT}/scripts/check-openapi-drift-suite-data-engineer.sh" >/dev/null
fi

echo "OpenAPI pre-commit checks passed."

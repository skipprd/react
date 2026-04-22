#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GIT_DIR="$(git -C "${REPO_ROOT}" rev-parse --git-dir)"
HOOK_PATH="${GIT_DIR}/hooks/pre-commit"
MARKER="skippr-react managed pre-commit hook"

mkdir -p "$(dirname "${HOOK_PATH}")"

if [[ -f "${HOOK_PATH}" ]] && ! grep -q "${MARKER}" "${HOOK_PATH}"; then
  backup_path="${HOOK_PATH}.backup.$(date +%Y%m%d%H%M%S)"
  cp "${HOOK_PATH}" "${backup_path}"
  echo "Backed up existing pre-commit hook to ${backup_path}"
fi

cat >"${HOOK_PATH}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
# skippr-react managed pre-commit hook
REPO_ROOT="$(git rev-parse --show-toplevel)"
exec bash "${REPO_ROOT}/.githooks/pre-commit"
EOF

chmod +x "${HOOK_PATH}"

echo "Installed pre-commit hook at ${HOOK_PATH}"
echo "The hook now runs versioned OpenAPI pre-commit checks from .githooks/pre-commit."

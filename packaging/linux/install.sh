#!/usr/bin/env bash
set -euo pipefail

PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
APP_DIR="${PREFIX}/share/applications"

if [[ "${1:-}" == "--remove" ]]; then
  rm -f "${BIN_DIR}/rover" "${APP_DIR}/rover.desktop"
  echo "Uninstalled Rover from ${PREFIX}."
  exit 0
fi

mkdir -p "${BIN_DIR}" "${APP_DIR}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

if [[ -f "${REPO_ROOT}/target/release/rover" ]]; then
  install -m 0755 "${REPO_ROOT}/target/release/rover" "${BIN_DIR}/rover"
fi
install -m 0644 "${SCRIPT_DIR}/rover.desktop" "${APP_DIR}/rover.desktop"
echo "Installed Rover to ${BIN_DIR}/rover"

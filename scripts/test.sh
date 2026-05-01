#!/usr/bin/env bash
# Quick smoke test in QEMU
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"${SCRIPT_DIR}/build.sh"
QEMU_RUN=1 "${SCRIPT_DIR}/build.sh"

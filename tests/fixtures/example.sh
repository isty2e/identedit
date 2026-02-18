#!/usr/bin/env bash
set -euo pipefail

process_data() {
  local value="$1"
  echo "$((value + 1))"
}

helper() {
  echo "helper"
}

if [[ "${1:-}" == "run" ]]; then
  process_data 3
fi

#!/usr/bin/env bash
# The app handles instance discovery and the platform-specific data directory.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
exec "$script_dir/opendictate-launch.sh" --toggle

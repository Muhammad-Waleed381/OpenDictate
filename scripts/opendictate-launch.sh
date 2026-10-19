#!/usr/bin/env bash
# Launch the installed app. OPENDICTATE_BIN can point to an unpackaged build.
set -euo pipefail
launcher="${OPENDICTATE_BIN:-${APPIMAGE:-opendictate}}"
exec "$launcher" "$@"

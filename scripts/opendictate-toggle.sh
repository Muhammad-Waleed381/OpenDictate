#!/usr/bin/env bash
# Toggle OpenDictate dictation from a GNOME custom keybinding.
set -u

SOCKET="$HOME/.local/share/com.opendictate.app/toggle.sock"
LAUNCHER="$(dirname "$0")/opendictate-launch.sh"

if [ -S "$SOCKET" ]; then
  timeout 2 python3 - "$SOCKET" <<'EOF'
import socket, sys
try:
    s = socket.socket(socket.AF_UNIX)
    s.connect(sys.argv[1])
    s.send(b"toggle")
    s.close()
except OSError:
    sys.exit(1)
EOF
  exit 0
fi

if [ -x "$LAUNCHER" ]; then
  nohup "$LAUNCHER" >/dev/null 2>&1 &
fi
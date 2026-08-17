#!/usr/bin/env bash
cd /home/waleed/Desktop/OpenDictate || exit 1
if pgrep -f "target/debug/opendictate" > /dev/null; then
  exit 0
fi
nohup npm run tauri dev > /tmp/opencode/tauri-dev.log 2>&1 &
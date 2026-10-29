#!/usr/bin/env bash
# Linux runtime check shared by Ubuntu and Alibaba Cloud Linux CI.
set -euo pipefail
binary=$(realpath "$1")
export CC_SWITCH_WEB_ROOT
CC_SWITCH_WEB_ROOT=$(realpath "$2")
export CC_SWITCH_TEST_HOME
CC_SWITCH_TEST_HOME=$(mktemp -d)
export CC_SWITCH_WEB_TOKEN
CC_SWITCH_WEB_TOKEN=$(openssl rand -hex 32)
export CC_SWITCH_WEB_BIND=127.0.0.1:18080
export DISPLAY= WAYLAND_DISPLAY=
server_pid=
cleanup() {
  if [[ -n $server_pid ]]; then
    kill -TERM "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf -- "$CC_SWITCH_TEST_HOME"
}
trap cleanup EXIT

ldd "$binary" > "$CC_SWITCH_TEST_HOME/linked-libraries"
if grep -Ei 'lib(gtk|gdk|webkit|javascriptcoregtk|soup|ayatana|appindicator)' \
  "$CC_SWITCH_TEST_HOME/linked-libraries"; then
  echo 'Web binary unexpectedly links desktop libraries' >&2
  exit 1
fi
# A host CLI fixture verifies executable discovery still works outside a desktop session.
mkdir -p "$CC_SWITCH_TEST_HOME/bin"
printf '#!/bin/sh\nprintf "99.0.0 (Claude Code)\\n"\n' > "$CC_SWITCH_TEST_HOME/bin/claude"
chmod +x "$CC_SWITCH_TEST_HOME/bin/claude"
export PATH="$CC_SWITCH_TEST_HOME/bin:$PATH"
log="$CC_SWITCH_TEST_HOME/server.log"
"$binary" > "$log" 2>&1 &
server_pid=$!
ready=false
for attempt in $(seq 1 60); do
  if curl --fail --silent --header "Authorization: Bearer $CC_SWITCH_WEB_TOKEN" \
    http://127.0.0.1:18080/api/session > /dev/null; then
    ready=true
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then cat "$log"; exit 1; fi
  sleep 1
done
if [[ $ready != true ]]; then cat "$log"; exit 1; fi
test "$(curl --silent --output /dev/null --write-out '%{http_code}' \
  http://127.0.0.1:18080/api/session)" = 401
curl --fail --silent http://127.0.0.1:18080/ > /dev/null
curl --fail --silent --max-time 60 --header "Authorization: Bearer $CC_SWITCH_WEB_TOKEN" \
  --header 'Content-Type: application/json' \
  --data '{"args":{"tools":["claude"]}}' \
  http://127.0.0.1:18080/api/command/get_tool_versions > "$CC_SWITCH_TEST_HOME/tools.json"
python3 -c 'import json,sys; tools=json.load(open(sys.argv[1])); assert len(tools)==1 and tools[0]["version"]=="99.0.0", tools' \
  "$CC_SWITCH_TEST_HOME/tools.json"
test -f "$CC_SWITCH_TEST_HOME/.cc-switch-web/cc-switch-web.db"
test ! -e "$CC_SWITCH_TEST_HOME/.cc-switch/cc-switch.db"
kill -TERM "$server_pid"
wait "$server_pid"
server_pid=

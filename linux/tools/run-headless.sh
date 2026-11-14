#!/usr/bin/env bash
# Run the Linux app with no display and a debug socket, for inspection by
# brickctl.py. State lives in $BRICK_RUN_DIR (default /tmp/brick-debug), so the
# real ~/.local/share/brick is never touched.
#
#   run-headless.sh start [--seed]   build if needed, launch, wait for the socket
#   run-headless.sh stop
#   run-headless.sh restart [--seed]
#   run-headless.sh log              show the app's output
#
# Set BRICK_PLATFORM=xcb to use a real X server (e.g. under xvfb-run) instead
# of Qt's offscreen platform.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"
run="${BRICK_RUN_DIR:-/tmp/brick-debug}"
sock="$run/debug.sock"
pidfile="$run/pid"
log="$run/app.log"
bin="$root/build/brick"

stop() {
  if [[ -f "$pidfile" ]] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    kill "$(cat "$pidfile")" 2>/dev/null || true
    for _ in $(seq 20); do kill -0 "$(cat "$pidfile")" 2>/dev/null || break; sleep 0.1; done
    kill -9 "$(cat "$pidfile")" 2>/dev/null || true
  fi
  rm -f "$pidfile" "$sock"
}

launch() {
  mkdir -p "$run/data" "$run/config"
  rm -f "$sock"
  [[ -x "$bin" ]] || cmake --build "$root/build"
  env \
    XDG_DATA_HOME="$run/data" XDG_CONFIG_HOME="$run/config" \
    BRICK_DEBUG_SOCKET="$sock" BRICK_SECRETS=memory \
    QT_QPA_PLATFORM="${BRICK_PLATFORM:-offscreen}" \
    QT_QPA_FONTDIR="${QT_QPA_FONTDIR:-/usr/share/fonts}" \
    LIBGL_ALWAYS_SOFTWARE=1 QT_QUICK_BACKEND=software \
    QTWEBENGINE_CHROMIUM_FLAGS="--no-sandbox --disable-gpu --in-process-gpu" \
    nohup "$bin" >>"$log" 2>&1 &
  echo $! >"$pidfile"
  for _ in $(seq 100); do
    [[ -S "$sock" ]] && return 0
    kill -0 "$(cat "$pidfile")" 2>/dev/null || { echo "app exited; see $log" >&2; exit 1; }
    sleep 0.1
  done
  echo "timed out waiting for $sock; see $log" >&2
  exit 1
}

case "${1:-start}" in
  start|restart)
    stop
    mkdir -p "$run"; : >"$log"
    if [[ "${2:-}" == "--seed" ]]; then
      # First run creates the database; seed it while the app is stopped.
      launch; stop
      python3 "$here/seed-demo.py" "$run/data/brick/brick/brick.sqlite"
    fi
    launch
    echo "running (pid $(cat "$pidfile")), socket $sock"
    ;;
  stop) stop ;;
  log) cat "$log" ;;
  *) sed -n '2,12p' "$0"; exit 2 ;;
esac

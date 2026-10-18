#!/bin/bash
# pk-pruefbus.sh – privater D-Bus-„Systembus“ mit packagekitd (Test-Backend) oder der
# PackageKit-Attrappe, ohne root, ohne den echten Systembus zu berühren.
#
#   pk-pruefbus.sh start [BACKEND] [--polkit erlauben|verweigern|verzoegern:MS|keiner]
#       BACKEND: test_thread (Standard) | test_fail | test_succeed | test_spawn | test_nop | attrappe
#   pk-pruefbus.sh stop | status | env
#
# Danach:  eval "$(pruefgeruest/pk-pruefbus.sh env)"   → setzt QS_PK_BUS für qs_paketkit.
# Ordner:  $QS_PK_DIR (Standard /tmp/qs-pk-$USER; kurz halten – Unix-Sockets ≤ 107 Zeichen).
# Das apt-Backend wird ausdrücklich abgelehnt: hier wird nie ein echtes Paket angefasst.
set -euo pipefail

HIER="$(cd "$(dirname "$0")" && pwd)"
WURZEL="$(dirname "$HIER")"
DIR="${QS_PK_DIR:-/tmp/qs-pk-$USER}"
PKD="${QS_PKD:-/usr/libexec/packagekitd}"
ATTRAPPE="${QS_PK_ATTRAPPE:-$WURZEL/target/debug/qs-pk-attrappe}"

laeuft() { [ -f "$DIR/$1.pid" ] && kill -0 "$(cat "$DIR/$1.pid")" 2>/dev/null; }

warte_auf_namen() { # $1 = Name
  for _ in $(seq 1 50); do
    if busctl --address="$(cat "$DIR/adresse")" status "$1" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  echo "Zeitüberschreitung: $1 erscheint nicht auf dem Prüfbus (Protokolle in $DIR)" >&2
  return 1
}

stop() {
  for d in daemon polkit bus; do
    if laeuft "$d"; then kill "$(cat "$DIR/$d.pid")" 2>/dev/null || true; fi
    rm -f "$DIR/$d.pid"
  done
  rm -f "$DIR/sock" "$DIR/adresse"
}

start() {
  local backend="${1:-test_thread}" polkit="erlauben"
  shift || true
  while [ $# -gt 0 ]; do
    case "$1" in
      --polkit) polkit="$2"; shift 2 ;;
      *) echo "Unbekannte Option: $1" >&2; exit 2 ;;
    esac
  done
  case "$backend" in
    test_thread|test_fail|test_succeed|test_spawn|test_nop|attrappe) ;;
    *) echo "Backend '$backend' abgelehnt – nur test_* oder attrappe (nie apt)." >&2; exit 2 ;;
  esac
  mkdir -p "$DIR"; chmod 700 "$DIR"
  stop
  if [ ! -x "$ATTRAPPE" ] && { [ "$backend" = attrappe ] || [ "$polkit" != keiner ]; }; then
    echo "Baue qs-pk-attrappe (einmalig) …" >&2
    (cd "$WURZEL" && nice cargo build -j2 --offline --bin qs-pk-attrappe >&2)
  fi
  cat > "$DIR/bus.conf" <<KONF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>system</type>
  <keep_umask/>
  <listen>unix:path=$DIR/sock</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow user="*"/>
    <allow own="*"/>
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
  </policy>
</busconfig>
KONF
  dbus-daemon --config-file="$DIR/bus.conf" --fork --print-address=3 --print-pid=4 3>"$DIR/adresse" 4>"$DIR/bus.pid"
  # GUID weglassen: sonst scheitert eine alte QS_PK_BUS nach einem Neustart am Handshake.
  local adr; adr="$(head -1 "$DIR/adresse")"; adr="${adr%%,guid=*}"; echo "$adr" > "$DIR/adresse"

  if [ "$polkit" != "keiner" ]; then
    [ -x "$ATTRAPPE" ] || { echo "Attrappe fehlt: $ATTRAPPE (cargo build --offline)" >&2; exit 1; }
    "$ATTRAPPE" polkit --adresse "$adr" --modus "$polkit" >"$DIR/polkit.log" 2>&1 &
    echo $! > "$DIR/polkit.pid"
    warte_auf_namen org.freedesktop.PolicyKit1
  fi

  if [ "$backend" = attrappe ]; then
    [ -x "$ATTRAPPE" ] || { echo "Attrappe fehlt: $ATTRAPPE" >&2; exit 1; }
    QS_ATTRAPPE_POLKIT="$( [ "$polkit" = verweigern ] && echo verweigern || echo erlauben )" \
      "$ATTRAPPE" paketkit --adresse "$adr" >"$DIR/daemon.log" 2>&1 &
  else
    [ -x "$PKD" ] || { echo "BLOCKED: $PKD fehlt" >&2; exit 1; }
    # --keep-environment ist Pflicht: ohne verwirft packagekitd DBUS_SYSTEM_BUS_ADDRESS
    # und versucht den echten Systembus (dort darf nur root den Namen besitzen).
    DBUS_SYSTEM_BUS_ADDRESS="$adr" "$PKD" --backend="$backend" --disable-timer --keep-environment \
      >"$DIR/daemon.log" 2>&1 &
  fi
  echo $! > "$DIR/daemon.pid"
  warte_auf_namen org.freedesktop.PackageKit
  echo "Prüfbus bereit: $adr  (Backend $backend, polkit $polkit)"
  echo "export QS_PK_BUS='$adr'"
}

status() {
  for d in bus polkit daemon; do
    if laeuft "$d"; then echo "$d: läuft (PID $(cat "$DIR/$d.pid"))"; else echo "$d: aus"; fi
  done
  [ -f "$DIR/adresse" ] && echo "QS_PK_BUS=$(cat "$DIR/adresse")" || true
}

case "${1:-}" in
  start) shift; start "$@" ;;
  stop) stop; echo "Prüfbus beendet." ;;
  status) status ;;
  env) [ -f "$DIR/adresse" ] && echo "export QS_PK_BUS='$(cat "$DIR/adresse")'" ;;
  *) sed -n '2,12p' "$0"; exit 2 ;;
esac

#!/bin/sh
# cycles.sh [cpu] [variants...]: cycles per body and per byte for each fixture,
# from perf stat cycles:u at two iteration counts (the difference over the
# gap), best of three. Runs target/release/wdxml; build it first.
set -e
cd "$(dirname "$0")"
CPU=${1:-12}
[ $# -gt 0 ] && shift
VARIANTS=${*:-parse-old parse-new map-old map-new pre}
PERF=$(nix-shell -p perf --run 'command -v perf')
one() {
  $PERF stat -x, -e cycles:u taskset -c "$CPU" ./target/release/wdxml run "$1" "$2" "$3" 2>&1 >/dev/null | cut -d, -f1
}
printf "%-18s %8s" fixture bytes
for v in $VARIANTS; do printf " %20s" "$v"; done
echo
for fx in multiget1-crref multiget500-lf multiget500-crlf multiget500-crref sync5000 propfind30; do
  case $fx in
    multiget1*) lo=2000 hi=12000 ;;
    propfind30) lo=200 hi=1200 ;;
    *) lo=20 hi=120 ;;
  esac
  bytes=$(./target/release/wdxml check | awk -v f="$fx" '$1 == f { print $2 }')
  printf "%-18s %8s" "$fx" "$bytes"
  for v in $VARIANTS; do
    best=
    for r in 1 2 3; do
      c=$(( ($(one "$v" "$fx" $hi) - $(one "$v" "$fx" $lo)) / (hi - lo) ))
      if [ -z "$best" ] || [ "$c" -lt "$best" ]; then best=$c; fi
    done
    printf " %11d %5.2fc/B" "$best" "$(echo "$best $bytes" | awk '{ print $1 / $2 }')"
  done
  echo
done

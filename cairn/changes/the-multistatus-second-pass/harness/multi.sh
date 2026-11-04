#!/usr/bin/env bash
# multi.sh <runs> "<binaries>" <variant/fixture>...: interleaved runs on
# CPU $CPU (12 by default), each run of a binary on the next of its three
# copies; per binary the median cycles per body (perf stat cycles:u at two
# iteration counts), its delta to the first binary, and the minimum.
H=$(cd "$(dirname "$0")" && pwd)
R=$(cd "$H/../../../.." && pwd)
W=$R/target/wdxml-ab
D=$R/target/wdxml-bins
PERF=$(nix-shell -p perf --run 'command -v perf' 2>/dev/null | tail -1)
CPU=${CPU:-12}
mkdir -p "$W/runs"
cd "$D" || exit 1
r=$1; bins=($2); shift 2
copies=(a b c)
one() {
  local v=${2%/*} fx=${2#*/} lo hi
  case $fx in
    multiget1*) lo=2000 hi=12000 ;;
    propfind30) lo=200 hi=1200 ;;
    *) lo=20 hi=120 ;;
  esac
  for n in $lo $hi; do
    taskset -c "$CPU" "$PERF" stat -x, -e cycles:u "./$1" run "$v" "$fx" $n 2>&1 >/dev/null | cut -d, -f1
  done | awk -v d=$((hi - lo)) 'NR==1{c=$1} NR==2{printf "%.0f\n", ($1-c)/d}'
}
for w in "$@"; do
  for b in "${bins[@]}"; do : > "$W/runs/$b.${w//\//_}"; done
  for i in $(seq "$r"); do
    for b in "${bins[@]}"; do one "$b.${copies[$(( (i - 1) % 3 ))]}" "$w" >> "$W/runs/$b.${w//\//_}"; done
  done
  line=$(printf "%-28s" "$w"); first=""
  for b in "${bins[@]}"; do
    f=$W/runs/$b.${w//\//_}
    med=$(sort -n "$f" | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    mn=$(sort -n "$f" | head -1)
    [ -z "$first" ] && first=$med
    line="$line $(awk -v m="$med" -v n="$mn" -v f="$first" 'BEGIN{printf "| %9d %+6.1f%% min %9d", m, (m-f)/f*100, n}')"
  done
  echo "$line"
done

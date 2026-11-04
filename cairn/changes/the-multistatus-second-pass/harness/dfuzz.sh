#!/bin/sh
# dfuzz.sh <bodies per seed> <first seed> [cpus...]: builds the differential
# fuzzer (diff/, against target/wdxml-ab/base-crate from base.sh) and runs one
# seed per CPU, 8 9 10 11 13 14 by default, off the measuring core.
H=$(cd "$(dirname "$0")" && pwd)
R=$(cd "$H/../../../.." && pwd)
cd "$H/diff" || exit 1
nix develop "$R" --command cargo build --release -q || exit 1
N=$1; S=$2; shift 2
CPUS=${*:-8 9 10 11 13 14}
i=0
for cpu in $CPUS; do
  taskset -c "$cpu" ./target/release/wddiff fuzz "$N" $((S + i)) > "target/f$i.log" 2>&1 &
  i=$((i + 1))
done
wait
cat target/f*.log
rm -f target/f*.log
ls fail-*.xml 2>/dev/null && exit 1
exit 0

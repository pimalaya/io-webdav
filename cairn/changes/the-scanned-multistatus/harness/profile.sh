#!/bin/sh
# profile.sh <variant> <fixture> [iterations]: callgrind over the measured
# calls only (wdxml::run, kept out of line), then instruction shares grouped
# by group.pl. Runs target/prof/wdxml; build it with --profile prof first.
set -e
cd "$(dirname "$0")"
out=target/cg-$1-$2.out
nix-shell -p valgrind --run "valgrind --tool=callgrind --toggle-collect='wdxml::run' --callgrind-out-file=$out ./target/prof/wdxml run $1 $2 ${3:-3}" >/dev/null 2>&1
nix-shell -p valgrind --run "callgrind_annotate --inclusive=no --threshold=100 --auto=no $out" | perl group.pl "$1/$2"
echo "full listing: callgrind_annotate --inclusive=no --auto=no $out"

#!/bin/sh
# cpuload.sh [seconds]: per-CPU busy share over an interval, to pick an idle
# measuring core (CPU n and its SMT sibling n+16 on this machine).
H=$(cd "$(dirname "$0")" && pwd)
W=$(cd "$H/../../../.." && pwd)/target/wdxml-ab
mkdir -p "$W"
grep '^cpu[0-9]' /proc/stat > "$W/stat1"
perl -e "select(undef, undef, undef, ${1:-2})"
grep '^cpu[0-9]' /proc/stat > "$W/stat2"
paste -d' ' "$W/stat1" "$W/stat2" | perl -ane '
  my @a = @F[1..10]; my @b = @F[12..21]; my ($t1, $t2) = (0, 0);
  $t1 += $_ for @a; $t2 += $_ for @b;
  my $dt = $t2 - $t1; my $idle = ($b[3] + $b[4]) - ($a[3] + $a[4]);
  printf "%s:%3.0f%% ", $F[0], $dt ? 100 * (1 - $idle / $dt) : 0;
  print "\n" if ++$n % 8 == 0;'

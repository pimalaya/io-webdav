#!/bin/sh
# base.sh [rev]: snapshots io-webdav at a git revision, or the working tree
# when none is given, as the baseline that build.sh and the differential
# fuzzer compare with: target/wdxml-ab/base, and target/wdxml-ab/base-crate
# renamed io-webdav-base.
set -e
H=$(cd "$(dirname "$0")" && pwd)
R=$(cd "$H/../../../.." && pwd)
W=$R/target/wdxml-ab
rm -rf "$W/base" "$W/base-crate"
mkdir -p "$W/base"
if [ -n "$1" ]; then
  git -C "$R" archive "$1" Cargo.toml Cargo.lock src | tar -x -C "$W/base"
else
  cp -r "$R/Cargo.toml" "$R/Cargo.lock" "$R/src" "$W/base/"
fi
cp -r "$W/base" "$W/base-crate"
sed -i 's/^name = "io-webdav"/name = "io-webdav-base"/' "$W/base-crate/Cargo.toml"
perl -0pi -e 's/\[\[example\]\]\n[^\[]*//' "$W/base-crate/Cargo.toml"
echo "baseline in $W/base"

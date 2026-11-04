#!/bin/sh
# build.sh <name> <base|tree> [profile]: builds the wdxml harness of change
# the-scanned-multistatus against the baseline (base.sh) or the working tree
# through one symlinked path, so both builds hash and lay out alike, and
# copies the binary three times to target/wdxml-bins, on ext4, for multi.sh.
# Profiles: release (O3) and small (opt-level "s", the Android app's).
set -e
H=$(cd "$(dirname "$0")" && pwd)
R=$(cd "$H/../../../.." && pwd)
S=$R/cairn/changes/the-scanned-multistatus/harness
W=$R/target/wdxml-ab
PROFILE=${3:-release}
case $2 in
  base) FROM=$W/base ;;
  tree) FROM=$R ;;
  *) echo "usage: build.sh <name> <base|tree> [profile]" >&2; exit 2 ;;
esac
mkdir -p "$W/h/src"
cp "$S"/src/*.rs "$W/h/src/"
cp "$S/Cargo.lock" "$W/h/"
sed -e "s|^io-webdav = { path = \"../../../..\"|io-webdav = { path = \"$W/cur\"|" \
  -e "s|^io-webdav-old = { path = \"target/io-webdav-old\"|io-webdav-old = { path = \"$S/target/io-webdav-old\"|" \
  "$S/Cargo.toml" > "$W/h/Cargo.toml"
printf '\n[profile.small]\ninherits = "release"\nopt-level = "s"\n' >> "$W/h/Cargo.toml"
exec 9>"$W/build.lock"
flock 9
ln -sfn "$FROM" "$W/cur"
cd "$W/h"
RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-loops=64" \
  nix develop "$R" --command cargo build -q --profile "$PROFILE" --target-dir "$W/target-$2"
D=$R/target/wdxml-bins
mkdir -p "$D"
for c in a b c; do cp "$W/target-$2/$PROFILE/wdxml" "$D/$1.$c"; done
sync
echo "built $1"

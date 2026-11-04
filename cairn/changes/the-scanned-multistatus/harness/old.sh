#!/bin/sh
# old.sh: extracts io-webdav as it was before the change (39bb548) into
# target/io-webdav-old, renamed so the harness can link it next to the
# current crate.
set -e
cd "$(dirname "$0")"
rm -rf target/io-webdav-old
mkdir -p target/io-webdav-old
git -C ../../../.. archive 39bb548 Cargo.toml src examples LICENSE-MIT LICENSE-APACHE \
  | tar -x -C target/io-webdav-old
sed -i 's/^name = "io-webdav"/name = "io-webdav-old"/; s/^version = "0.6.0"/version = "0.5.99"/' \
  target/io-webdav-old/Cargo.toml

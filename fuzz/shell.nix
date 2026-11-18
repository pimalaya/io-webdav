# A dedicated fuzzing shell: a nightly toolchain (cargo-fuzz needs the `-Z`
# sanitizer flags stable rustc rejects) plus cargo-fuzz itself, via fenix, so no
# rustup and no nix-ld shim are needed. Enter it with:
#
#   nix-shell fuzz/shell.nix --run "cargo fuzz run parse"
{
  pkgs ? import <nixpkgs> { },
  fenix ? import (fetchTarball "https://github.com/nix-community/fenix/archive/monthly.tar.gz") { },
}:

pkgs.mkShell {
  packages = with pkgs; [
    fenix.default.toolchain
    cargo-fuzz
  ];

  # The libFuzzer/ASan binary cargo-fuzz builds links libstdc++ dynamically, and
  # NixOS has no global libstdc++.so.6; point the loader at the toolchain's copy.
  shellHook = ''
    export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  '';
}

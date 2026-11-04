# The stable toolchain of the flake.lock with the aarch64, armv7 and i686 musl
# std, linked by rust-lld and run under qemu: nix-shell cross.nix --run "...".
{
  lock ? builtins.fromJSON (builtins.readFile ../../../../flake.lock),
  locked ? name: with lock.nodes.${name}.locked; builtins.getFlake "github:${owner}/${repo}/${rev}",
  system ? builtins.currentSystem,
  pkgs ? (locked "nixpkgs").legacyPackages.${system},
  fenix ? (locked "fenix").packages.${system},
}:

pkgs.mkShell {
  packages = [
    (fenix.combine [
      fenix.stable.rustc
      fenix.stable.cargo
      fenix.stable.clippy
      fenix.targets.aarch64-unknown-linux-musl.stable.rust-std
      fenix.targets.armv7-unknown-linux-musleabihf.stable.rust-std
      fenix.targets.i686-unknown-linux-musl.stable.rust-std
    ])
    pkgs.qemu
  ];
  CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = "rust-lld";
  CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER = "qemu-aarch64 -cpu neoverse-n1";
  CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER = "rust-lld";
  CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER = "qemu-arm -cpu cortex-a15";
  CARGO_TARGET_I686_UNKNOWN_LINUX_MUSL_LINKER = "rust-lld";
  CARGO_TARGET_I686_UNKNOWN_LINUX_MUSL_RUNNER = "qemu-i386";
}

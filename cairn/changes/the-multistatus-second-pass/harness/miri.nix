# A nightly with Miri from the fenix of the flake.lock, for the lanes crate.
{
  lock ? builtins.fromJSON (builtins.readFile ../../../../flake.lock),
  locked ? name: with lock.nodes.${name}.locked; builtins.getFlake "github:${owner}/${repo}/${rev}",
  system ? builtins.currentSystem,
  pkgs ? (locked "nixpkgs").legacyPackages.${system},
  fenix ? (locked "fenix").packages.${system},
}:

pkgs.mkShell {
  packages = [
    (fenix.latest.withComponents [
      "cargo"
      "miri"
      "rust-src"
      "rustc"
    ])
  ];
}

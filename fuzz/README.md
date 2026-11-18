# Fuzzing

Coverage-guided fuzzing of the server-side parsers with [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer). cargo-fuzz needs a nightly toolchain; on NixOS the dedicated fuzz/shell.nix provides both (nightly via fenix plus cargo-fuzz):

```sh
nix-shell fuzz/shell.nix --run "cd fuzz && cargo fuzz run server_bodies corpus/server_bodies seeds"
```

## Targets

- `server_bodies`: arbitrary bytes never panic a request body or header parser of the `server` module, every raw element a `PROPPATCH`, `MKCOL` or `MKCALENDAR` keeps reads back to the same name, text and children once wrapped in a fresh body, and a multistatus written out of those elements reads back through the client multistatus parser.

The seeds/ folder holds hand-written bodies to start from; libFuzzer writes its corpus into the first directory, corpus/server_bodies, which stays untracked.

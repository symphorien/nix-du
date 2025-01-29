# Installation
Here are some installation methods.

## From nixpkgs

If the version you get this way is too old, the two following methods will give you access to the latest version.

`nix-du` is available in `nixpkgs >= 18.09-pre`, see the badge below.

[![Packaging status](https://repology.org/badge/vertical-allrepos/nix-du.svg)](https://repology.org/metapackage/nix-du)

If the channel you follow is not recent enough:
```
nix-env -f channel:nixos-unstable -iA nix-du
```

## Directly from the repository
### Build requirements

`nix-du` is only tested to build on the latest NixOS stable version and NixOS unstable.
Notably, this implies you need a less than six month old rust version. Currently
`nix-du` is known to need at least `rustc >= 1.82`. Semantic versionning is
slightly fuzzy with executables (as opposed to libraries) but bumping the
minimum required version of `rustc` is not considered a breaking change.

You need `nix` version 2 and `boost` (a dependency of `nix`). `lix` can be used instead of `nix`.
Tests need `dot` in `$PATH`. Tests are known to non-deterministically fail with
`nix < 2.1`.

Note that `nix` 2 is only needed to build `nix-du`; `nix-du` should be able to talk to a
`nix` 1 daemon.

### With nix
```
$ nix-env -if https://github.com/symphorien/nix-du/archive/master.tar.gz
```
This is tested to work on the current stable release of nixpkgs and `nixos-unstable`.
If your channel is older, you can use `nixos-unstable` instead this way:
```
$ nix-env -I nixpkgs=https://nixos.org/channels/nixos-unstable/nixexprs.tar.xz -if https://github.com/symphorien/nix-du/archive/master.tar.gz
```
### With `cargo`

Run `cargo build --release` at the root of the repository.

To get all dependencies in scope, at the root of the repository, run
`nix-shell`.

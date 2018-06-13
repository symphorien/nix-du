map (url: with import (builtins.fetchTarball url) {};
# this function patches nix to fix an issue causing spurious test failures
# FIXME: remove when the channel contains the patch
let fix2223 = nix: nix.overrideAttrs(old: {
  doInstallCheck = false;
  patches = [ (fetchpatch {
    url = https://patch-diff.githubusercontent.com/raw/NixOS/nix/pull/2223.patch;
    sha256 = "0ykmqas4qsqj3xhb26x8i9711k5sb0x84g779ms3zp083ckpdbhf";
  }) ];
});
in
  map (x: callPackage ./nix-du.nix { nix = fix2223 x; }) [ nixStable nixUnstable ]
) [ channel:nixos-18.03 channel:nixpkgs-unstable ]

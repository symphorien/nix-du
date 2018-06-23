map (url: with import (builtins.fetchTarball url) {};
# this function patches nix to fix an issue causing spurious test failures
# FIXME: remove when the channel contains the patch
let fix2223 = nix: callPackage ./fix.nix { inherit nix; }; in
  map (x: callPackage ./nix-du.nix { nix = fix2223 x; }) [ nixStable nixUnstable ]
) [ channel:nixos-18.03 channel:nixpkgs-unstable ]

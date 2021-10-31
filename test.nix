map (url: with import (builtins.fetchTarball url) {};
  map (x: callPackage ./nix-du.nix { nix = x; }) [ nixStable nixUnstable ]
) [ channel:nixos-20.09 channel:nixpkgs-unstable ]

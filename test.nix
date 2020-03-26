map (url: with import (builtins.fetchTarball url) {};
  map (x: callPackage ./nix-du.nix {}) [ nixStable nixUnstable ]
) [ channel:nixos-20.03 channel:nixpkgs-unstable ]

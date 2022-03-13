map
  (url: with import (builtins.fetchTarball url) { };
  map (x: (callPackage ./nix-du.nix { nix = x; }).tested) [ nixStable /* nixUnstable */ ]
  ) [ /* channel:nixos-21.11 */ channel:nixpkgs-unstable ]

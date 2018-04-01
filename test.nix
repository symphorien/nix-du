with import <nixpkgs> {};
map (x: callPackage ./nix-du.nix { nix = x; }) [ nixStable nixUnstable ]

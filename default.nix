with import <nixpkgs> {};
callPackage ./nix-du.nix { nix = callPackage ./fix.nix { nix = nixStable; }; }

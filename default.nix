let sources = import nix/sources.nix {}; in
let pkgs = import sources.nixpkgs {}; in
pkgs.callPackage ./nix-du.nix { nix = pkgs.nixVersions.nix_2_28; }

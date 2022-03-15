let
  isDerivation = x: (x.type or null) == "derivation";
  allNixVersions = pkgs: [ pkgs.nixStable pkgs.nixUnstable ] ++ (builtins.filter isDerivation (builtins.attrValues (pkgs.nixVersions or { })));
in
map
  (url:
    let
      pkgs = import (builtins.fetchTarball url) { };
    in
    map (x: (pkgs.callPackage ./nix-du.nix { nix = x; }).tested) (allNixVersions pkgs)
  ) [ /* channel:nixos-21.11 */ channel:nixpkgs-unstable ]

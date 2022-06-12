let
  isDerivation = x: (x.type or null) == "derivation";
  allNixVersions = pkgs: [ pkgs.nixStable pkgs.nixUnstable ] ++ (builtins.filter isDerivation (builtins.attrValues (pkgs.nixVersions or { })));
in
map
  (url:
    let
      pkgs = import (builtins.fetchTarball url) { };
      channel_name = builtins.replaceStrings [ "channel:" ] [ "" ] (builtins.toString url);
    in
    map (x: (pkgs.callPackage ./nix-du.nix { nix = x; }).tested.overrideAttrs (old: { name = old.name + "-${channel_name}-nix-(${x.version}"; })) (allNixVersions pkgs)
  ) [ channel:nixos-22.05 channel:nixpkgs-unstable ]

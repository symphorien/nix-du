let
  isDerivation = x: (x.type or null) == "derivation";
  tryEvalOpt = x: let res = builtins.tryEval x; in if res.success then res.value else null;
  allNixVersions = pkgs: [ pkgs.nixStable pkgs.nixUnstable ] ++ (builtins.filter isDerivation (map tryEvalOpt (builtins.attrValues (pkgs.nixVersions or { }))));
in
map
  (url:
    let
      pkgs = import (builtins.fetchTarball url) { };
      channel_name = builtins.replaceStrings [ "channel:" ] [ "" ] (builtins.toString url);
    in
    map (x: (pkgs.callPackage ./nix-du.nix { nix = x; }).tested.overrideAttrs (old: { name = old.name + "-${channel_name}-nix-(${x.version}"; })) (allNixVersions pkgs)
  ) [ "https://github.com/NixOS/nixpkgs/archive/352ff3f22631470832d5ad1b197d4aeaa614058c.tar.gz" ]

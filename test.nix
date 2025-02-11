let
  isDerivation = x: (x.type or null) == "derivation";
  tryEvalOpt = x: let res = builtins.tryEval x; in if res.success then res.value else null;
  allNixVersions = pkgs: [ pkgs.nix pkgs.lix ] ++ (builtins.filter isDerivation (map tryEvalOpt (builtins.attrValues (pkgs.nixVersions or { }))));
  channelsToTest = [ "channel:nixos-24.11" "channel:nixos-unstable" ];
in
map
  (url:
    let
      pkgs = import (builtins.fetchTarball url) { };
      channel_name = builtins.replaceStrings [ "channel:" ] [ "" ] (builtins.toString url);
    in
    map (x: (pkgs.callPackage ./nix-du.nix { nix = x; }).tested.overrideAttrs (old: { name = old.name + "-${channel_name}-${x.name}"; })) (allNixVersions pkgs)
  ) channelsToTest

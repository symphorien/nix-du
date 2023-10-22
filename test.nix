let
  isDerivation = x: (x.type or null) == "derivation";
  tryEvalOpt = x: let res = builtins.tryEval x; in if res.success then res.value else null;
  allNixVersions = pkgs: [ pkgs.nixStable pkgs.nixUnstable ] ++ (builtins.filter isDerivation (map tryEvalOpt (builtins.attrValues (pkgs.nixVersions or { }))));
  isDarwin = (builtins.match ".*darwin.*" builtins.currentSystem) != null;
  channelsToTest = if isDarwin then
    [ "channel:nixos-unstable" ] # buildRustCrate is broken on 23.05 due to stripping rlibs
  else
    [ "channel:nixos-23.05" "channel:nixos-unstable" ];
in
map
  (url:
    let
      pkgs = import (builtins.fetchTarball url) { };
      channel_name = builtins.replaceStrings [ "channel:" ] [ "" ] (builtins.toString url);
    in
    map (x: (pkgs.callPackage ./nix-du.nix { nix = x; }).tested.overrideAttrs (old: { name = old.name + "-${channel_name}-nix-(${x.version}"; })) (allNixVersions pkgs)
  ) channelsToTest

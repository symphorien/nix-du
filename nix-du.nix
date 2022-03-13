{ callPackage, pkgs, lib, graphviz, nix, nlohmann_json, defaultCrateOverrides, xcbuild, pkg-config, boost, darwin, stdenv, rustPlatform }:
let
  cargo = import ./Cargo.nix {
    inherit pkgs;
    defaultCrateOverrides = defaultCrateOverrides // {
      nix-du = attrs: {
        buildInputs = [
          boost
          nix
          nlohmann_json
        ] ++ lib.optional stdenv.isDarwin darwin.apple_sdk.frameworks.Security;
        nativeBuildInputs = [
          pkg-config
          rustPlatform.bindgenHook
        ] ++ lib.optional stdenv.isDarwin xcbuild;
      };
    };
  };
  nix-du-untested = cargo.rootCrate.build;
  nix-du-tested = nix-du-untested.override {
    runTests = true;
    testInputs = [ graphviz nix ];
  };
in
# this hack allows to use inputsFrom in mkShell
nix-du-untested // { tested = nix-du-tested; }

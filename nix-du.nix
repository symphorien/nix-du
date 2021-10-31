{ callPackage, pkgs, lib, graphviz, nix, nlohmann_json, defaultCrateOverrides, xcbuild, pkg-config, boost, darwin, stdenv }:
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
        ] ++ lib.optional stdenv.isDarwin xcbuild;
      };
    };
  };
in
cargo.rootCrate.build.override {
  runTests = true;
  testInputs = [ graphviz nix ];
}

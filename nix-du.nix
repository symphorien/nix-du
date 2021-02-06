{ callPackage, lib, graphviz, nix, defaultCrateOverrides, xcbuild, pkg-config, boost, darwin, stdenv }:
let
  cargo = callPackage ./Cargo.nix {
    defaultCrateOverrides = defaultCrateOverrides // {
      nix-du = attrs: {
        buildInputs = [
          boost
          nix
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

{ callPackage, graphviz, nix, defaultCrateOverrides, strace, boost, darwin, stdenv }:
let
  cargo = callPackage ./Cargo.nix {
    defaultCrateOverrides = defaultCrateOverrides // {
      nix-du = attrs: {
        buildInputs = [
          boost
          nix
        ] ++ stdenv.lib.optional stdenv.isDarwin darwin.apple_sdk.frameworks.Security;
      };
    };
  };
in
cargo.rootCrate.build.override {
  runTests = true;
  testInputs = [ graphviz nix ];
}

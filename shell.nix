((import ./default.nix).override(_: {source = null;})).overrideAttrs(old: {
  nativeBuildInputs = old.nativeBuildInputs ++ [ /*(import <nixpkgs> {}).rust-bindgen*/ ];
})

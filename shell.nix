with import <nixpkgs> {};
mkShell {
  buildInputs = [ nix boost ];
  nativeBuildInputs = [ pkg-config nlohmann_json graphviz cargo rustc rust-bindgen rls rustfmt cargo-outdated ];

  RUST_BACKTRACE=1;
}

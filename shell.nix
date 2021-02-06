with import <nixpkgs> {};
mkShell {
  buildInputs = [ nix boost ];
  nativeBuildInputs = [ pkg-config graphviz cargo rustc rust-bindgen rls rustfmt cargo-outdated ];
}

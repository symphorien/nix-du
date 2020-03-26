with import <nixpkgs> {};
mkShell {
  buildInputs = [ nix boost ];
  nativeBuildInputs = [ graphviz cargo rustc rust-bindgen rls rustfmt cargo-outdated ];
}

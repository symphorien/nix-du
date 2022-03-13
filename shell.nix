let
  sources = import nix/sources.nix {};
  pkgs = import sources.nixpkgs {};
  nix-du = pkgs.callPackage ./nix-du.nix {};
in
pkgs.mkShell {
  inputsFrom = [ nix-du nix-du.tested ];
  nativeBuildInputs = with pkgs; [ rust-bindgen rls rustfmt cargo-outdated ];

  RUST_BACKTRACE=1;
}

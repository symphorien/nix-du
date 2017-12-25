with import <nixpkgs> {}; 
stdenv.mkDerivation {
  name = "nix-du-build-env";
  buildInputs = [
    llvm
    cargo
    boost
    nix
  ];
  CLANG_PATH="${clang}/bin/clang";
  LIBCLANG_PATH="${llvmPackages.clang-unwrapped}/lib";
  LD_LIBRARY_PATH="${llvmPackages.clang-unwrapped}/lib";
  RUST_BACKTRACE=1;
}

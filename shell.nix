with import <nixpkgs> {}; 
let buildInputs = [
    boost
    nix
  ]; in
/*
nix-du is built with bindgen, which in turn uses the (unwrapped) libclang.so.
It allows to pass the (missing) flags to include the libc and lib(std)cxx as
an environment variable.
This derivation is a file containing these flags.

We use clang because with gcc the path to libstdcxx is not found...
*/
let bindgenFlags = clangStdenv.mkDerivation {
  name = "nix-du-bindgen-flags";
  inherit buildInputs;
  # FIXME: fragile...
  buildCommand = ''
  NIX_DEBUG=1 clang++ -x c++ /dev/null -c -o /dev/null  |& awk 'BEGIN { print_next=0; } { if (print_next) { print $0; print_next=0; } } /-i/ { print_next = 1; print $0; }' > $out
  '';
}; in
stdenv.mkDerivation rec {
  name = "nix-du-build-env";
  inherit buildInputs;
  nativeBuildInputs = [
    llvm
    cargo
  ];
  CLANG_PATH="${clang}/bin/clang";
  LIBCLANG_PATH="${llvmPackages.clang-unwrapped.lib}/lib";
  LD_LIBRARY_PATH="${llvmPackages.clang-unwrapped.lib}/lib";
  RUST_BACKTRACE=1;
  preConfigure = ''
    export BINDGEN_EXTRA_CFLAGS="$(cat ${bindgenFlags})";
  '';
  shellHook = preConfigure;
}

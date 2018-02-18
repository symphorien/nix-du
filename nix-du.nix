{ stdenv, fetchFromGitHub,
rustPlatform, nix, boost, 
clangStdenv, clang, llvmPackages,
graphviz }:
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
let src =
  with stdenv.lib.sources;
  let filter = name: type:
  let filename = baseNameOf (toString name); in
  !(type == "directory" && filename == "target");
  in
    cleanSourceWith { inherit filter; src = cleanSource ./.; };
in
rustPlatform.buildRustPackage rec {
  name = "nix-du-${version}";
  version = "0.1.0";

  inherit src;

  # must be changed when Cargo.lock is modified
  cargoSha256 = "1r2025p6ih7rkhv8wy992n8cwa4k8cs33r7cxvkr6azkwn0v756n";

  doCheck = true;
  checkInputs = [ graphviz ];
  nativeBuildInputs = [] ++ stdenv.lib.optionals doCheck checkInputs;

  inherit buildInputs;
  
  /* bindgen stuff */
  CLANG_PATH="${clang}/bin/clang";
  LIBCLANG_PATH="${llvmPackages.clang-unwrapped.lib}/lib";
  LD_LIBRARY_PATH="${llvmPackages.clang-unwrapped.lib}/lib";
  preConfigure = ''
    export BINDGEN_EXTRA_CFLAGS="$(cat ${bindgenFlags})";
  '';
  shellHook = preConfigure;

  meta = with stdenv.lib; {
    description = "A tool to determine which gc-roots take space in your nix store";
    homepage = https://github.com/symphorien/nix-du;
    license = licenses.lgpl3;
    maintainers = [ maintainers.symphorien ];
    platforms = platforms.all;
  };
}

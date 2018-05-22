{ stdenv, fetchFromGitHub,
rustPlatform, nix, boost, 
clangStdenv, clang, llvmPackages,
graphviz,
cargoSha256 ? "04c48lzi7hny3nq4ffdpvsr4dxbi32faka163fp1yc9953zdw9az",
source ?
  with stdenv.lib.sources;
  let filter = name: type:
  let filename = baseNameOf (toString name); in
  !(type == "directory" && filename == "target");
  in
    cleanSourceWith { inherit filter; src = cleanSource ./.; }
}:
rustPlatform.buildRustPackage rec {
  name = "nix-du-${version}";
  version = "0.1.1";

  src = source;
  inherit cargoSha256;

  doCheck = true;
  checkInputs = [ graphviz ];
  nativeBuildInputs = [] ++ stdenv.lib.optionals doCheck checkInputs;

  buildInputs = [
    boost
    nix
  ];

  RUST_BACKTRACE=1;

  meta = with stdenv.lib; {
    description = "A tool to determine which gc-roots take space in your nix store";
    homepage = https://github.com/symphorien/nix-du;
    license = licenses.lgpl3;
    maintainers = [ maintainers.symphorien ];
    platforms = platforms.all;
  };
}

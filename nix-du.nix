{ stdenv, fetchFromGitHub,
rustPlatform, nix, boost, 
clangStdenv, clang, llvmPackages,
graphviz,
source ?
  with stdenv.lib.sources;
  let filter = name: type:
  let filename = baseNameOf (toString name); in
  !(type == "directory" && filename == "target");
  in
  {
    src = cleanSourceWith { inherit filter; src = cleanSource ./.; };
    # must be changed when Cargo.lock is modified
    cargoSha256 = "04c48lzi7hny3nq4ffdpvsr4dxbi32faka163fp1yc9953zdw9az";
  }
}:
rustPlatform.buildRustPackage rec {
  name = "nix-du-${version}";
  version = "0.1.0";

  inherit (source) src cargoSha256;

  doCheck = true;
  checkInputs = [ graphviz ];
  nativeBuildInputs = [] ++ stdenv.lib.optionals doCheck checkInputs;

  buildInputs = [
    boost
    nix
  ];

  meta = with stdenv.lib; {
    description = "A tool to determine which gc-roots take space in your nix store";
    homepage = https://github.com/symphorien/nix-du;
    license = licenses.lgpl3;
    maintainers = [ maintainers.symphorien ];
    platforms = platforms.all;
  };
}

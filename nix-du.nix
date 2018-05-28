{ stdenv, fetchFromGitHub,
rustPlatform, nix, boost, 
clangStdenv, clang, llvmPackages,
graphviz,
cargoSha256 ? "0qq7a6ncxnbjvnmly99awqrk9f3z9b55ifil7b0bn5yhk4h9sa6y",
source ?
  with stdenv.lib.sources;
  let filter = name: type:
  let filename = baseNameOf (toString name); in
  #builtins.trace "type ${type} filename ${filename} name ${name}" (
  !(type == "directory" && (filename == "target" || filename == "screenshots")) &&
  !(type == "regular" && builtins.match ''.*\.(md|bk)'' filename != null) &&
  !(type == "symlink" && filename == "result") &&
  !(type == "regular" && builtins.match ''.*\.nix'' filename != null && builtins.match ''.*/tests/nix/.*'' name == null) &&
  !(builtins.match ''\..*'' filename != null)
  #&& (builtins.trace "ok" true))
  ;
  in
    cleanSourceWith { inherit filter; src = cleanSource ./.; }
}:
let
  cargotoml = builtins.readFile ./Cargo.toml;
  reg = ''.*[[]package[]][^]]*version *= *"([^"]*)".*'';
  matches = builtins.match reg cargotoml;
  version = builtins.head matches;
in

rustPlatform.buildRustPackage rec {
  name = "nix-du-${version}";
  inherit version;

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

{ stdenv, fetchFromGitHub, writeTextFile,
rustPlatform, nix, boost, 
clang_6,
graphviz, darwin,
cargoSha256 ? "0sva4lnhccm6ly7pa6m99s3fqkmh1dzv7r2727nsg2f55prd4kxc",
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
  # otherwise, when source=null, buildRustPackage will still try to run cargo-vendor
  cargoVendorDir = if source == null then (writeTextFile {name="dummy"; text="";}) else null;
  inherit cargoSha256;

  doCheck = true;
  checkInputs = [ graphviz ];
  # nix 2.2 uses std::experimental::optional which is removed in clang7
  nativeBuildInputs = stdenv.lib.optional stdenv.cc.isClang [ clang_6 ];

  buildInputs = [
    boost
    nix
  ] ++ stdenv.lib.optional stdenv.isDarwin darwin.apple_sdk.frameworks.Security;

  RUST_BACKTRACE=1;

  meta = with stdenv.lib; {
    description = "A tool to determine which gc-roots take space in your nix store";
    homepage = https://github.com/symphorien/nix-du;
    license = licenses.lgpl3;
    maintainers = [ maintainers.symphorien ];
    platforms = platforms.unix;
  };
}

((import ./default.nix).override(_: {source = null;})).overrideAttrs(old: {
  nativeBuildInputs = (old.nativeBuildInputs or []) ++ (with (import <nixpkgs> {}); [ rust-bindgen ]);
})

#!/bin/sh

cat > src/bindings.rs <<EOF
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
EOF

nixver="$(pkg-config --modversion nix-main | cut -d. -f 1,2 | tr . 0)"

bindgen \
    --impl-debug \
    --whitelist-function populateGraph \
    --whitelist-type path_t \
    --opaque-type 'std::.*' \
    wrapper.hpp \
    -- -x c++ $(pkg-config --cflags nix-store nix-main) -DNIXVER="$nixver" \
    >> src/bindings.rs

#!/bin/sh

cat > src/bindings.rs <<EOF
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
EOF

bindgen \
    --impl-debug \
    --whitelist-function populateGraph \
    --whitelist-type path_t \
    --opaque-type 'std::.*' \
    wrapper.hpp \
    -- -std=c++14 -x c++ \
    >> src/bindings.rs

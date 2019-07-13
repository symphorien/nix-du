// SPDX-License-Identifier: LGPL-3.0

extern crate cc;

fn main() {
    // this build script only depends on the wrapper
    println!("cargo:rerun-if-changed=wrapper.hpp");

    // try compiling with nix <= or > 2.2
    [true, false]
        .iter()
        .filter_map(|&nix_2_3_or_greater| {
            let mut builder = cc::Build::new();
            builder
                .cpp(true) // Switch to C++ library compilation.
                .opt_level(2) // needed for fortify hardening included by nix
                .file("wrapper.cpp");
            if nix_2_3_or_greater {
                builder
                    .flag("-std=c++17")
                    .define("FINDROOTS_HAS_CENSOR", None)
                    .define("ROOTS_ARE_MAP_TO_SET", None);
            } else {
                builder.flag("-std=c++14");
            }
            builder.try_compile("libnix_adapter.a").err()
        })
        // panic if both compilations failed
        .nth(1)
        .iter()
        .for_each(|second_err| panic!("{:?}", second_err));

    /* must be passed as an argument to the linker *after* -lnix_adapter */
    println!("cargo:rustc-link-lib=nixstore");
    println!("cargo:rustc-link-lib=nixmain");
    println!("cargo:rustc-link-lib=nixutil");
}

// SPDX-License-Identifier: LGPL-3.0

extern crate cc;

fn main() {
    // this build script only depends on the wrapper
    println!("cargo:rerun-if-changed=wrapper.hpp");

    cc::Build::new()
        .cpp(true) // Switch to C++ library compilation.
        .flag("-std=c++14")
        .flag("-O2")
        .file("wrapper.cpp")
        .compile("libnix_adapter.a");

    /* must be passed as an argument to the linker *after* -lnix_adapter */
    println!("cargo:rustc-link-lib=nixstore");
    println!("cargo:rustc-link-lib=nixmain");
    println!("cargo:rustc-link-lib=nixutil");
}

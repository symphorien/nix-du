// SPDX-License-Identifier: LGPL-3.0

extern crate bindgen;
extern crate cc;
extern crate shlex;

use std::env;
use std::path::PathBuf;

fn main() {
    // this build script only depends on the wrapper
    println!("cargo:rerun-if-changed=wrapper.hpp");


    let mut builder = bindgen::Builder::default()
        .clang_arg("-std=c++11")
        .header("wrapper.hpp")
        .whitelist_type("path_t")
        .whitelist_function("populateGraph")
        .opaque_type("std::.*")
        .impl_debug(true);

    if let Ok(cflags) = env::var("BINDGEN_EXTRA_CFLAGS") {
        let extra_args = shlex::split(&cflags).expect("Cannot parse $BINDGEN_EXTRA_CFLAGS");
        builder = builder.clang_args(extra_args);
    }

    let bindings = builder.generate().expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    cc::Build::new()
        .cpp(true) // Switch to C++ library compilation.
        .file("wrapper.cpp")
        .compile("libnix_adapter.a");

    /* must be passed as an argument to the linker *after* -lnix_adapter */
    println!("cargo:rustc-link-lib=nixstore");
    println!("cargo:rustc-link-lib=nixmain");
}

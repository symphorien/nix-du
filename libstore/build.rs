extern crate bindgen;
extern crate cc;
extern crate shlex;

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-lib=nixstore");
    println!("cargo:rustc-link-lib=nixmain");

    let mut builder = bindgen::Builder::default()
        .clang_arg("-std=c++11")
        .header("wrapper.hpp")
        .whitelist_type("nix::LocalStore")
        .whitelist_type("nix::RemoteStore")
        .whitelist_type("nix::PathSet")
        .whitelist_type("nix::Path")
        .whitelist_type("nix_adapter::.*")
        .whitelist_function("nix::initNix")
        .whitelist_function("nix::openStore")
        .whitelist_function("nix_adapter::.*")
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

}

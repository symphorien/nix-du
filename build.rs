extern crate bindgen;
extern crate cc;

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-lib=nixstore");
    println!("cargo:rustc-link-lib=nixmain");

    let bindings = bindgen::Builder::default()
        .clang_args(vec!(
"-std=c++11",
"-idirafter"
,"/nix/store/mqalq0v2laqblw00dp7pwwckj2ra6jyh-glibc-2.26-75-dev/include"
,"-isystem"
,"/nix/store/c30dlkmiyrjxxjv6nv63igjkzcj1fzxi-gcc-6.4.0/include/c++/6.4.0"
,"-isystem"
,"/nix/store/c30dlkmiyrjxxjv6nv63igjkzcj1fzxi-gcc-6.4.0/include/c++/6.4.0/x86_64-unknown-linux-gnu"
, "-isystem"
,"/nix/store/lzv2dd0wrjf8d3c4nlfhcgl668jwvdri-boost-1.65.1-dev/include"
, "-isystem"
,"/nix/store/cqhdk51xqxj1990v20y3wfnvhr0r8yds-nix-1.11.15-dev/include"
  ))
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
        .impl_debug(true)
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    cc::Build::new()
        .cpp(true) // Switch to C++ library compilation.
        .file("wrapper.cpp")
        .compile("libnix_adapter.a");

}

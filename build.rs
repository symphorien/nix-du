// SPDX-License-Identifier: LGPL-3.0

fn main() {
    // this build script only depends on the wrapper
    println!("cargo:rerun-if-changed=wrapper.hpp");
    println!("cargo:rerun-if-changed=wrapper.cpp");

    // find which version of nix we have
    let nix = pkg_config::Config::new()
        .atleast_version("2.2")
        .probe("nix-main")
        .unwrap();
    println!("Found nix version {}", &nix.version);

    // compile libnix_adapter.a
    let mut builder = cc::Build::new();
    builder
        .cpp(true) // Switch to C++ library compilation.
        .opt_level(2) // needed for fortify hardening included by nix
        .file("wrapper.cpp");
    if nix.version.as_str() >= "2.3" {
        builder.flag("-std=c++17");
    } else {
        builder.flag("-std=c++14");
    }
    let version = if nix.version.as_str() >= "2.4" {
        204
    } else if nix.version.as_str() >= "2.3" {
        203
    } else {
        202
    };
    builder.define("NIXVER", version.to_string().as_str());
    builder.compile("libnix_adapter.a");

    /* must be passed as an argument to the linker *after* -lnix_adapter */
    pkg_config::Config::new()
        .atleast_version("2.2")
        .probe("nix-store")
        .unwrap();
    pkg_config::Config::new()
        .atleast_version("2.2")
        .probe("nix-main")
        .unwrap();
}

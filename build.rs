use std::path::PathBuf;
// SPDX-License-Identifier: LGPL-3.0

fn v(s: &str) -> versions::Versioning {
    versions::Versioning::new(s).unwrap_or_else(|| panic!("could not parse version {}", s))
}

fn main() {
    // this build script only depends on the wrapper
    println!("cargo:rerun-if-changed=wrapper.hpp");
    println!("cargo:rerun-if-changed=wrapper.cpp");

    // find which version of nix we have
    let nix = pkg_config::Config::new()
        .atleast_version("2.2")
        .probe("nix-main")
        .unwrap();
    eprintln!("Found nix version {}", &nix.version);
    let nix_version = v(&nix.version);

    // compile libnix_adapter.a
    let mut builder = cc::Build::new();
    builder
        .cpp(true) // Switch to C++ library compilation.
        .opt_level(2) // needed for fortify hardening included by nix
        .file("wrapper.cpp");
    let standard = if nix_version >= v("2.3") {
        "-std=c++17"
    } else {
        "-std=c++14"
    };
    builder.flag(standard);
    let version = if nix_version >= v("2.8") {
        208usize
    } else if nix_version >= v("2.7") {
        207usize
    } else if nix_version >= v("2.4") {
        204
    } else if nix_version >= v("2.3") {
        203
    } else if nix_version >= v("2.2") {
        202
    } else {
        eprintln!("warning: could not compare version {nix_version} to known nix versions, attempting nix 2.8 wrapper");
        208
    };
    eprintln!("building with NIXVER={version}");
    builder.define("NIXVER", version.to_string().as_str());
    builder.compile("libnix_adapter.a");

    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.hpp")
        .allowlist_function("populateGraph")
        .allowlist_type("path_t")
        .opaque_type("std::.*")
        .clang_arg(format!("-DNIXVER={}", version))
        .clang_arg(standard)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

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

v0.4.0

* add --dump option for debugging
* take keep-outputs and keep-derivations into account in nix.conf
* nix 2.4 support

v0.3.3:

* forgot to update Cargo.lock with previous release

v0.3.2:

* fix roots in /proc not being aggregated as transient roots

v0.3.1:

* support for nix 2.3

v0.3.0:

* add --root option to reduce the scope of the analysis to the transitive closure of a store path

v0.2.0:

* add -O option to take store optimisation into account
* do not add a root for transient roots if there is not transient root to begin with.
* do not inhibit ^C in the rust part of nix-du

v0.1.2:

* temporary roots are now aggregated with memory roots.
* fix sandboxed builds on darwin.
* fix a bug when querying info about not-yet-built temporary gc-roots.
* Only keep a gc-root after filtering when it has a filtered in child.
* `nix-du` now prints a small summary of the size of the store, use `-q` to disable.

v0.1.1

* `bindgen` is no longer a build-time dependency. Bindings are committed to the repo.

v0.1.0

* initial version

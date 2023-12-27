v1.2.0:
* fix build on darwin
* support nix 2.19

v1.1.1:
* fix build on darwin

v1.1.0:

* make quotient algorithm more memory efficient
* improvements to the progress bar

v1.0.0:

* show a human readable description of gc roots along with their age when possible
* transitive reduction is now built-in, it is not necessary anymore to pipe the
  output to tred

v0.6.0:

* update some dependencies. The CLI parsing may have unintentionally changed.
* parallelize store optimization handling
* fix build with nix 2.10pre

v0.5.1:

* fix bad system call error in tests in pkgsi686Linux.nix-du
* fix build with nix 2.8

v0.5.0:

* Fix build with nix 2.7
* Fix build on 32bit architectures by generating bindings at build time instead
  of committing them. rustPlatform.bindgenHook is now a build dependency.

v0.4.1

* Fix running tests on darwin

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

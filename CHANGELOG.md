master:

* temporary roots are now aggregated with memory roots.
* fix sandboxed builds on darwin.
* fix a bug when querying info about non-yet-built temporary gc-roots.
* Only keep a gc-root after filtering when it has a filtered in child.

v0.1.1

* `bindgen` is no longer a build-time dependency. Bindings are committed to the repo.

v0.1.0

* initial version

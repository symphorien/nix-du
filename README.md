# nix-du
`nix-du` is a tool aimed at helping answer the following question:

What gc-roots should I remove in my nix store to free some space ?

## Getting started
### Building
It is enough to have `nix` (1.11) installed.
```
$ git clone https://github.com/symphorien/nix-du
$ cd nix-du
$ nix-shell
$ cargo build --release
$ target/release/nix-du --help # it works :)
```

### Running
`nix-du` generates a directed graph (more on that later) in the DOT format.
Therefore you need `dot` installed (it is usually available under the package name `graphviz`).
Then you can translate the graph in various more "traditional" image formats.

For example:
```sh
# to svg
nix-du -n 60 | dot -Tsvg > store.svg
# to png; use tred to remove a few more edges
nix-du -n 60 | tred | dot -Tpng > store.png
```
Another option is to use an interactive viewer such as `zgrviewer`
```sh
nix-du -n 60 > store.dot
zgrviewer store.dot
```
### Interpreting
TODO: insert an example here

On the left are the gc-roots. The other nodes are labeled with a package name, but it has little meaning. What
matters is their size. Blue means "lightest"; red means "heaviest".
An edge from A to B means "you won't be able to remove B as long as A is live". If you remove all
incoming edges of a node, it _should_ go away when you run `nix-collect-garbage` and this _should_ free approximately
the displayed amount of space.

## FAQ
### What is _really_ this graph ?
If you use neither `-s` nor `-n` then the output graph is derived from the reference graph of your store as followed
* the set of nodes is the quotient of the original node set by the relation "these two nodes are (recursively) referenced
by the same set of gc-roots"
* There is an edge between two classes if it is not a self loop and there was an edge between any elements of the classes
in the original graph

The representent of the class inherits the total size of the class and the name of an arbitary member.
This is sometimes useful, but also often meaningless. For example I have already seen a huge node `glibc-locales` with 
an edge to texlive components which is surprising since `glibc-locales` has no references...

If you use any of `-s` (only keep nodes bigger than a given size) or `-n` (only keep the `n` biggest nodes) then an approximation
is done so results may be less accurate (but far more readable !)

### But my store is far havier than displayed ?!
Only live paths are displayed.

### I asked for 60 nodes with `-n 60` but I got 120 !?
gc-roots are always kept in the final graph.

### I removed a huge node and yet `nix-collect-garbage` freed only little space
For now `nix-du` does not take hard linked files (see `nix-store --optimise`) into account which means that if they belong
to 3 derivations they will be counted 3 times.

## Limitations
* for now the ffi stuff is leaky
* only connection to the nix-daemon is implemented, sorry to users of single user installs
* no optimised store support

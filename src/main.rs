extern crate petgraph;
extern crate libstore;

mod depgraph;

use petgraph::dot::{Dot, Config};

fn main() {
    libstore::init_nix();
    let mut store = libstore::Store::new();
    let g = depgraph::store_to_depinfos(&mut store);
    eprintln!("The graph before has n={}, m={}", g.graph.node_count(), g.graph.edge_count());
    let g = depgraph::condense(g);
    eprintln!("The graph after has n={}, m={}", g.graph.node_count(), g.graph.edge_count());
    println!("{:?}", Dot::with_config(&g.graph, &[Config::EdgeNoLabel]));
}

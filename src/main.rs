extern crate petgraph;

mod libstore;
mod depgraph;

use petgraph::dot::{Dot, Config};

fn main() {
    libstore::init_nix();
    let mut store = libstore::Store::new();
    let g = depgraph::store_to_depgraph(&mut store);
    println!("{:?}", Dot::with_config(&g, &[Config::EdgeNoLabel]));
}

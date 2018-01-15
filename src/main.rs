extern crate petgraph;
extern crate libstore;

mod depgraph;
mod dot;
use std::io;

fn main() {
    libstore::init_nix();
    let mut store = libstore::Store::new();
    let g = depgraph::store_to_depinfos(&mut store);
    eprintln!(
        "The graph before has n={}, m={}",
        g.graph.node_count(),
        g.graph.edge_count()
    );
    let g = depgraph::condense(g);
    eprintln!(
        "The graph after has n={}, m={}",
        g.graph.node_count(),
        g.graph.edge_count()
    );
    let g = depgraph::condense_exact(g);
    eprintln!(
        "The graph after² has n={}, m={}",
        g.graph.node_count(),
        g.graph.edge_count()
    );
    let g = depgraph::keep(g, &|d| d.size > 50_000_000);
    eprintln!(
        "The graph after³ has n={}, m={}",
        g.graph.node_count(),
        g.graph.edge_count()
    );

    {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        dot::render(&g, &mut handle).expect("Cannot write to stdout");
    }

}

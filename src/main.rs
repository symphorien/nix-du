// SPDX-License-Identifier: LGPL-3.0

#[macro_use]
extern crate clap;
extern crate human_size;
extern crate petgraph;

pub mod depgraph;
pub mod dot;
pub mod reduction;
pub mod bindings;
use std::io;
use human_size::Size;

/* so that these functions are available in libnix_adepter.a */
pub use depgraph::{register_node, register_edge};

fn main() {
    let matches = clap::App::new("nix-du")
        .about(
            "visualise what gc-roots you should delete to free space in your nix-store",
        )
        .long_about(
            "
This program outputs a graph on stdout in the dot format which may help you figuring out which
gc-roots should be removed in order to reclaim space in the nix store.

To get started, just run `nix-du -n 60 | tred | dot -Tsvg > /tmp/blah.svg` and then view the result
in a browser or dedicated software like zgrviewer.

The exact meaning of the graph is as follows: if you use neither -s nor -n then a node is the
equivalence class of all store paths on which the exact same set of gc-roots depend. The size is
meant to be accurate, but the label is that of an arbitrary store path of this equivalence class.
An arrow from A to B means that to get rid of B you have to get rid of A before.
",
        )
        .version(crate_version!())
        .arg(
            clap::Arg::with_name("min-size")
                .short("s")
                .long("min-size")
                .value_name("SIZE")
                .help(
                    "Hide nodes below this size (a unit should be specified: -s=\"50 MB\")",
                )
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("nodes")
                .short("n")
                .long("nodes")
                .value_name("N")
                .conflicts_with("min-size")
                .help(
                    "Only keep the approximately N biggest nodes (union gc-roots)",
                )
                .takes_value(true),
        )
        .get_matches();

    let mut min_size = match matches.value_of("min-size") {
        Some(min_size_str) => {
            min_size_str
                .parse::<Size>()
                .unwrap_or_else(|_| {
                    clap::Error::value_validation_auto(
    "The argument to --min-size is not a valid syntax. Try -s=\"5 MB\" for example."
    .to_owned()).exit()
                })
                .into_bytes() as u64
        }
        None => 0,
    };
    let n_nodes = match matches.value_of("nodes") {
        Some(min_size_str) => {
            match min_size_str.parse::<usize>() {
                Ok(x) if x > 0 => x,
                _ => {
                    clap::Error::value_validation_auto(
                        "The argument to --nodes is not a positive integer".to_owned(),
                    ).exit()
                }
            }
        }
        None => 0,
    };

    let mut g = depgraph::DepInfos::read_from_store();
    eprintln!(
        "The graph before has n={}, m={}",
        g.graph.node_count(),
        g.graph.edge_count()
    );
    g = reduction::condense(g);
    eprintln!(
        "The graph after has n={}, m={}",
        g.graph.node_count(),
        g.graph.edge_count()
    );

    if n_nodes > 0 && n_nodes < g.graph.node_count() {
        let mut sizes: Vec<u64> = g.graph.raw_nodes().iter().map(|n| n.weight.size).collect();
        sizes.sort_unstable();
        min_size = sizes[sizes.len().saturating_sub(n_nodes)];
    }

    if min_size > 0 {
        g = reduction::keep(g, &|d| d.size >= min_size);
        eprintln!(
            "The graph after³ has n={}, m={}",
            g.graph.node_count(),
            g.graph.edge_count()
        );
    }

    {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        dot::render(&g, &mut handle).expect("Cannot write to stdout");
    }
}

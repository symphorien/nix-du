extern crate petgraph;

use libstore;
use std::collections;
use std::vec::Vec;

type Derivation = libstore::PathId;
type Edge = ();

pub type DepGraph = petgraph::graph::Graph<Derivation, Edge>;

pub fn store_to_depgraph(store: &mut libstore::Store) -> DepGraph {
    let mut valid_paths = store.valid_paths();
    let mut g = DepGraph::with_capacity(valid_paths.len(), valid_paths.len());
    let mut path_to_node = collections::HashMap::with_capacity(valid_paths.len());
    let mut queue = Vec::new();
    for pe in valid_paths {
        let path = pe.to_path(store);
        let node = g.add_node(path.id());
        path_to_node.insert(path.id(), node);
        queue.push((node, path));
    }
    while !queue.is_empty() {
        let (node, path) = queue.pop().unwrap();
        for dep in path.deps(store) {
            let child = dep.to_path(store);
            let entry = path_to_node.entry(child.id());
            let childnode =
                match entry {
                    collections::hash_map::Entry::Vacant(e) => {
                        let new_node = g.add_node(child.id());
                        e.insert(new_node);
                        queue.push((new_node, child));
                        new_node
                    },
                    collections::hash_map::Entry::Occupied(e) => *e.get()
                };
            g.add_edge(node, childnode, ());
        }
    }
    g
}



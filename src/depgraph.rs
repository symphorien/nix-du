extern crate petgraph;

use libstore;
use std::collections;
use std::vec::Vec;
use std::ffi::CString;

type Derivation = CString;
type Edge = ();

pub type DepGraph = petgraph::graph::Graph<Derivation, Edge>;

pub struct DepInfos {
    pub graph: DepGraph,
    pub roots: Vec<petgraph::graph::NodeIndex>
}

pub fn store_to_depinfos(store: &mut libstore::Store) -> DepInfos {
    let mut valid_paths = store.valid_paths();
    let mut g = DepGraph::with_capacity(valid_paths.len(), valid_paths.len());
    let mut path_to_node = collections::HashMap::with_capacity(valid_paths.len());
    let mut queue = Vec::new();
    for pe in valid_paths {
        let path = pe.to_path(store);
        let node = g.add_node(path.path().to_owned());
        path_to_node.insert(path.path().to_owned(), node);
        queue.push((node, path));
    }
    while !queue.is_empty() {
        let (node, path) = queue.pop().unwrap();
        for dep in path.deps() {
            let child = dep.to_path(store);
            let entry = path_to_node.entry(child.path().to_owned());
            let childnode =
                match entry {
                    collections::hash_map::Entry::Vacant(e) => {
                        let new_node = g.add_node(child.path().to_owned());
                        e.insert(new_node);
                        queue.push((new_node, child));
                        new_node
                    },
                    collections::hash_map::Entry::Occupied(e) => *e.get()
                };
            g.add_edge(node, childnode, ());
        }
    }
    
    let roots_it = store.roots();
    let mut roots = Vec::with_capacity(roots_it.len());
    for (link, path) in roots_it {
        let destnode = path_to_node[path.to_path(store).path()];
        let fromnode = g.add_node(link);
        g.add_edge(fromnode, destnode, ());
        roots.push(fromnode);
    }

    g.shrink_to_fit();
    DepInfos{ graph: g, roots }
}



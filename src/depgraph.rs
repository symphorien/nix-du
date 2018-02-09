// SPDX-License-Identifier: LGPL-3.0

extern crate petgraph;
extern crate memchr;

use std::vec::Vec;
use std::ffi::CString;
use std::collections;
use libstore;

use petgraph::prelude::NodeIndex;

#[derive(Debug, Clone)]
pub struct Derivation {
    pub path: CString,
    pub size: u64,
    pub is_root: bool,
}

impl Derivation {
    /// Note: clones the string describing the path.
    pub fn new_inner(p: &libstore::Path) -> Self {
        Derivation {
            path: p.path().clone(),
            size: p.size(),
            is_root: false,
        }
    }
    pub fn new_root(p: CString) -> Self {
        let size = p.to_bytes().len() as u64; // good approximation for symlinks
        Derivation {
            path: p,
            size: size,
            is_root: true,
        }
    }
    pub fn dummy() -> Self {
        Derivation {
            path: CString::default(),
            size: 0,
            is_root: false,
        }
    }

    pub fn name(&self) -> &[u8] {
        let whole = &self.path.to_bytes();
        if self.is_root {
            whole
        } else {
            match memchr::memrchr(b'/', whole) {
                None => whole,
                Some(i) => {
                    let whole = &whole[i + 1..];
                    match memchr::memchr(b'-', whole) {
                        None => whole,
                        Some(i) => &whole[i + 1..],
                    }
                }
            }
        }
    }
}

pub type Edge = ();

pub type DepGraph = petgraph::graph::Graph<Derivation, Edge, petgraph::Directed>;

#[derive(Debug, Clone)]
pub struct DepInfos {
    pub graph: DepGraph,
    pub roots: Vec<NodeIndex>,
}

pub fn store_to_depinfos(store: &mut libstore::Store) -> DepInfos {
    let valid_paths = store.valid_paths();
    let mut g = DepGraph::with_capacity(valid_paths.len(), valid_paths.len());
    let mut path_to_node = collections::HashMap::with_capacity(valid_paths.len());
    let mut queue = Vec::new();
    for pe in valid_paths {
        let path = pe.to_path(store);
        let node = g.add_node(Derivation::new_inner(&path));
        path_to_node.insert(path.path().clone(), node);
        queue.push((node, path));
    }
    while !queue.is_empty() {
        let (node, path) = queue.pop().unwrap();
        for dep in path.deps() {
            let child = dep.to_path(store);
            //eprintln!("{:?} child of {:?}", child.path(), path.path());
            let entry = path_to_node.entry(child.path().clone());
            let childnode = match entry {
                collections::hash_map::Entry::Vacant(e) => {
                    let new_node = g.add_node(Derivation::new_inner(&child));
                    e.insert(new_node);
                    queue.push((new_node, child));
                    new_node
                }
                collections::hash_map::Entry::Occupied(e) => *e.get(),
            };
            g.add_edge(node, childnode, ());
        }
    }

    let roots_it = store.roots();
    let mut roots = Vec::with_capacity(roots_it.len());
    for (link, path) in roots_it {
        let destnode = path_to_node[path.to_path(store).path()];
        let fromnode = g.add_node(Derivation::new_root(link));
        g.add_edge(fromnode, destnode, ());
        roots.push(fromnode);
    }

    g.shrink_to_fit();
    DepInfos { graph: g, roots }
}

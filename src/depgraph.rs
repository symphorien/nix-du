// SPDX-License-Identifier: LGPL-3.0

extern crate memchr;
extern crate petgraph;

use std::vec::Vec;
use std::ffi::CStr;
use std::os::raw::c_void;
use std::collections;
use bindings;

use petgraph::prelude::NodeIndex;
use petgraph::visit::IntoNodeReferences;
use petgraph::visit::NodeRef;
use petgraph::visit::VisitMap;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Derivation {
    pub path: Vec<u8>,
    pub size: u64,
    pub is_root: bool,
}

impl Derivation {
    /// Note: clones the string describing the path.
    unsafe fn new(p: &bindings::path_t) -> Self {
        Derivation {
            path: CStr::from_ptr(p.path).to_bytes().iter().cloned().collect(),
            size: p.size,
            is_root: p.is_root != 0,
        }
    }

    pub fn dummy() -> Self {
        Derivation {
            path: vec![],
            size: 0,
            is_root: false,
        }
    }

    /// Return `blah` when the path of the
    /// derivation is `/nix/store/<hash>-blah`
    /// In case of failure, may return a bigger
    /// slice of the path.
    pub fn name(&self) -> &[u8] {
        let whole = &self.path;
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

    pub fn is_transient_root(&self) -> bool {
        self.path.starts_with(b"{memory:") || self.path.starts_with(b"{temp:")
    }
}

pub type Edge = ();

pub type DepGraph = petgraph::graph::Graph<Derivation, Edge, petgraph::Directed>;

#[derive(Debug, Clone)]
pub struct DepInfos {
    pub graph: DepGraph,
    pub roots: Vec<NodeIndex>,
}

// symbol exported to libnix_adapter
#[no_mangle]
pub extern "C" fn register_node(g: *mut DepGraph, p: *const bindings::path_t) {
    let p: &bindings::path_t = unsafe { p.as_ref().unwrap() };
    let g: &mut DepGraph = unsafe { g.as_mut().unwrap() };
    let drv = unsafe { Derivation::new(p) };
    g.add_node(drv);
}

// symbol exported to libnix_adapter
#[no_mangle]
pub extern "C" fn register_edge(g: *mut DepGraph, from: u32, to: u32) {
    let g: &mut DepGraph = unsafe { g.as_mut().unwrap() };
    g.add_edge(NodeIndex::from(from), NodeIndex::from(to), ());
}

impl DepInfos {
    /// returns the dependency graph of the nix-store
    /// actual connection specifics are left to libnixstore
    /// (reading ourselves, connecting to a daemon...)
    pub fn read_from_store() -> Result<Self, i32> {
        let mut g = DepGraph::new();
        let gptr = &mut g as *mut _ as *mut c_void;
        let res = unsafe { bindings::populateGraph(gptr) };

        if res == 0 {
            Ok(DepInfos::new_from_graph(g))
        } else {
            Err(res)
        }
    }

    /// given a `DepGraph`, build the `root` attr of
    /// the corresponding `DepInfos` and return it
    pub fn new_from_graph(g: DepGraph) -> Self {
        let roots = g.node_references()
            .filter_map(|(idx, drv)| if drv.is_root { Some(idx) } else { None })
            .collect();

        let di = DepInfos { graph: g, roots };
        debug_assert!(di.roots_attr_coherent());
        di
    }

    /// returns the sum of the size of all the derivations reachable from a root
    pub fn reachable_size(&self) -> u64 {
        let mut dfs = petgraph::visit::Dfs::empty(&self.graph);
        let mut sum = 0;
        for &idx in &self.roots {
            dfs.discovered.visit(idx);
            dfs.stack.push(idx);
        }
        while let Some(idx) = dfs.next(&self.graph) {
            sum += self.graph[idx].size;
        }
        sum
    }
    /// returns the set of paths of the roots
    /// intended for testing mainly
    #[cfg(test)]
    pub fn roots_name(&self) -> collections::BTreeSet<Vec<u8>> {
        self.roots
            .iter()
            .map(|&idx| &self.graph[idx].path)
            .cloned()
            .collect()
    }
    /// returns wether di.roots is really the set of indices of root nodes
    /// according to `drv.is_root` and according to the graph structure
    /// intended for tests mainly
    pub fn roots_attr_coherent(&self) -> bool {
        let from_nodes: collections::BTreeSet<NodeIndex> = self.graph
            .node_references()
            .filter_map(|nref| if nref.weight().is_root {
                Some(nref.id())
            } else {
                None
            })
            .collect();
        let from_attr: collections::BTreeSet<NodeIndex> = self.roots.iter().cloned().collect();
        let from_structure: collections::BTreeSet<NodeIndex> = self.graph
            .externals(petgraph::Direction::Incoming)
            .collect();
        from_attr == from_nodes && from_nodes.is_subset(&from_structure)
    }
}

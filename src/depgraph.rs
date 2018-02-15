// SPDX-License-Identifier: LGPL-3.0

extern crate memchr;
extern crate petgraph;

use std::vec::Vec;
use std::ffi::{CString, CStr};
use std::os::raw::c_void;
use bindings;

use petgraph::prelude::NodeIndex;
use petgraph::visit::IntoNodeReferences;

#[derive(Debug, Clone)]
pub struct Derivation {
    pub path: CString,
    pub size: u64,
    pub is_root: bool,
}

impl Derivation {
    /// Note: clones the string describing the path.
    unsafe fn new(p: &bindings::path_t) -> Self {
        Derivation {
            path: CStr::from_ptr(p.path).to_owned(),
            size: p.size,
            is_root: p.is_root != 0,
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

#[no_mangle]
pub extern "C" fn register_node(g: *mut DepGraph, p: *const bindings::path_t) {
    let p: &bindings::path_t = unsafe { p.as_ref().unwrap() };
    let g: &mut DepGraph = unsafe { g.as_mut().unwrap() };
    let drv = unsafe { Derivation::new(p) };
    g.add_node(drv);
}

#[no_mangle]
pub extern "C" fn register_edge(g: *mut DepGraph, from: u32, to: u32) {
    let g: &mut DepGraph = unsafe { g.as_mut().unwrap() };
    g.add_edge(NodeIndex::from(from), NodeIndex::from(to), ());
}

pub fn get_depinfos() -> DepInfos {
    let mut g = DepGraph::new();
    let gptr = &mut g as *mut _ as *mut c_void;
    unsafe { bindings::populateGraph(gptr) };

    let roots = g.node_references()
        .filter_map(|(idx, drv)| if drv.is_root { Some(idx) } else { None })
        .collect();

    DepInfos { graph: g, roots }
}

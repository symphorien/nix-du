// SPDX-License-Identifier: LGPL-3.0

extern crate memchr;
extern crate petgraph;
extern crate fixedbitset;

use std::vec::Vec;
use std::ffi::{CStr, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::os::raw::c_void;
use std::borrow::Cow;
use std::fmt;
#[cfg(test)]
use std::collections;
use std;
use bindings;

use petgraph::prelude::NodeIndex;
use petgraph::visit::IntoNodeReferences;
use petgraph::visit::Dfs;

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum NodeKind {
    Path,
    Link,
    Dummy,
    FilteredOut,
    Memory,
    Temporary,
    Transient,
    Shared,
}

impl NodeKind {
    pub fn is_gc_root(self) -> bool {
        use self::NodeKind::*;
        match self {
            Transient | Link | Memory | Temporary => true,
            FilteredOut | Path | Shared | Dummy => false,
        }
    }

    pub fn is_transient(self) -> bool {
        use self::NodeKind::*;
        match self {
            Memory | Temporary => true,
            Transient | Link | FilteredOut | Path | Shared | Dummy => false,
        }
    }
}

pub type Path = Vec<u8>;

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum NodeDescription {
    /// A real, valid store path
    Path(Path),
    /// A indirect root, as a link on the filesystem
    Link(Path),
    /// A dummy node, for example the fake root whose all gc roots are children
    Dummy,
    /// A node gathering all filtered out ones
    FilteredOut,
    /// A node gathering all Memory and Temporary roots
    Transient,
    /// An in-memory root
    Memory(Path),
    /// A temporary root
    Temporary(Path),
    /// Symbolises a set of inodes de-duplicated by store optimisation
    Shared(Path),
}

const SHARED_PREFIX: &'static [u8] = b"shared:";

impl NodeDescription {
    /// Return `blah` when the path of the
    /// derivation is `/nix/store/<hash>-blah`
    /// In case of failure, may return a bigger
    /// slice of the path.
    pub fn name(&self) -> Cow<[u8]> {
        use self::NodeDescription::*;
        match self {
            Path(path) => {
                let whole = &path;
                let inner = match memchr::memrchr(b'/', whole) {
                    None => whole,
                    Some(i) => {
                        let whole = &whole[i + 1..];
                        match memchr::memchr(b'-', whole) {
                            None => whole,
                            Some(i) => &whole[i + 1..],
                        }
                    }
                };
                Cow::Borrowed(inner)
            },
            Link(path) | Memory(path) | Temporary(path) => Cow::Borrowed(&path),
            Dummy => Cow::Borrowed(b"{dummy}"),
            FilteredOut => Cow::Borrowed(b"{filtered out}"),
            Transient => Cow::Borrowed(b"{transient}"),
            Shared(name) => {
                let mut res = Vec::with_capacity(SHARED_PREFIX.len()+name.len());
                res.extend(SHARED_PREFIX);
                res.extend(name);
                Cow::Owned(res)}
            ,
        }
    }

    /// returns the path as an `OsStr` if this node is on the filesystem
    pub fn path_as_os_str(&self) -> Option<&OsStr> {
        use self::NodeDescription::*;
        match self {
            Link(path) | Path(path) => Some(OsStr::from_bytes(path)),
            _ => None
        }
    }

    pub fn path(&self) -> Option<&Path> {
        use self::NodeDescription::*;
        match self {
            Link(path) | Path(path) | Memory(path) | Temporary(path) =>  Some(&path),
            Shared(name) => Some(&name),
            Transient | Dummy | FilteredOut => None
        }
    }

    pub fn kind(&self) -> NodeKind {
        use self::NodeDescription::*;
        match self {
            Path(_) => NodeKind::Path,
            Link(_) => NodeKind::Link,
            Memory(_) => NodeKind::Memory,
            Temporary(_) => NodeKind::Temporary,
            Shared(_) => NodeKind::Shared,
            Dummy => NodeKind::Dummy,
            FilteredOut => NodeKind::FilteredOut,
            Transient => NodeKind::Transient,
        }
    }
}

impl fmt::Debug for NodeDescription {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let p = match self.path() {
            Some(x) => x.as_slice(),
            None => b"",
        };
        let p = String::from_utf8_lossy(p);
        write!(f, "{:?}({})", self.kind(), p)
    }
}

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct DepNode {
    pub description: NodeDescription,
    pub size: u64,
}

impl DepNode {
    /// Note: clones the string describing the path.
    unsafe fn new(p: &bindings::path_t) -> Self {
        let path: Vec<u8> = CStr::from_ptr(p.path).to_bytes().iter().cloned().collect();
        use self::NodeDescription::*;
        let description;
        if path[0] == b'/' {
            if p.is_root != 0 {
                description = Link(path);
            } else {
                description = Path(path);
            }
        } else if path.starts_with(b"{memory:") {
            description = Memory(path);
        } else if path.starts_with(b"{temp:") {
            description = Temporary(path);
        } else {
            panic!("Unknown store path type: {}", String::from_utf8_lossy(&path));
        }
        Self {
            description,
            size: p.size,
        }
    }

    pub fn dummy() -> Self {
        DepNode {
            description: NodeDescription::Dummy,
            size: 0,
        }
    }

    pub fn kind(&self) -> NodeKind {
        self.description.kind()
    }

    pub fn name(&self) -> Cow<[u8]> {
        self.description.name()
    }
}

impl fmt::Debug for DepNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "N({:?}, size={})",
            self.description,
            self.size
        )
    }
}

pub type Edge = ();

pub type DepGraph = petgraph::graph::Graph<DepNode, Edge, petgraph::Directed>;

#[derive(Debug, Clone)]
pub struct DepInfos {
    pub graph: DepGraph,
    pub root: NodeIndex,
}

// symbol exported to libnix_adapter
#[no_mangle]
pub extern "C" fn register_node(g: *mut DepGraph, p: *const bindings::path_t) {
    let p: &bindings::path_t = unsafe { p.as_ref().unwrap() };
    let g: &mut DepGraph = unsafe { g.as_mut().unwrap() };
    let drv = unsafe { DepNode::new(p) };
    g.add_node(drv);
}

// symbol exported to libnix_adapter
#[no_mangle]
pub extern "C" fn register_edge(g: *mut DepGraph, from: u32, to: u32) {
    if from == to {
        return;
    }
    let g: &mut DepGraph = unsafe { g.as_mut().unwrap() };
    g.add_edge(NodeIndex::from(from), NodeIndex::from(to), ());
}

impl DepInfos {
    /// returns the dependency graph of the nix-store
    /// actual connection specifics are left to libnixstore
    /// (reading ourselves, connecting to a daemon...)
    pub fn read_from_store(root: Option<OsString>) -> Result<Self, i32> {
        let mut g = DepGraph::new();
        let gptr = &mut g as *mut _ as *mut c_void;
        let root_data = root.map(|path| {
            let mut bytes = path.into_vec();
            bytes.push(0);
            bytes
        });
        let rootptr: *const u8 = match root_data.as_ref() {
            None => std::ptr::null(),
            Some(path) => path.as_ptr()
        };
        let res = unsafe { bindings::populateGraph(gptr, rootptr as *const std::os::raw::c_char) };

        if res != 0 {
            return Err(res);
        }
        let root_idx = match &root_data {
            None => g.add_node(DepNode::dummy()),
            Some(_) => NodeIndex::from(0)
        };
        let mut di = DepInfos {
            root: root_idx,
            graph: g,
        };
        if root_data.is_none() {
            let gc_roots: Vec<_> = di.graph.node_references().filter_map(|(idx, n)| if n.kind().is_gc_root() { Some(idx) } else {None}).collect();
            for root in gc_roots {
                di.graph.add_edge(di.root, root, ());
            }
        }
        Ok(di)
    }

    /// returns the sum of the size of all the derivations reachable from the root
    pub fn reachable_size(&self) -> u64 {
        let mut dfs = self.dfs();
        let mut sum = 0;
        while let Some(idx) = dfs.next(&self.graph) {
            sum += self.graph[idx].size;
        }
        sum
    }

    /// returns a Dfs suitable to visit all reachable nodes.
    pub fn dfs(&self) -> Dfs<NodeIndex, fixedbitset::FixedBitSet> {
        petgraph::visit::Dfs::new(&self.graph, self.root)
    }

    /// Returns the iterator of roots
    pub fn roots(&self) -> petgraph::graph::Neighbors<(), u32> {
        self.graph.neighbors(self.root)
    }


    /// returns the set of paths of the roots
    /// intended for testing mainly
    #[cfg(test)]
    pub fn roots_name(&self) -> collections::BTreeSet<String> {
        self.roots()
            .map(|idx| { assert_ne!(idx, self.root); String::from_utf8_lossy(&self.graph[idx].name()).into() })
            .collect()
    }
}

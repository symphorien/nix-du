// SPDX-License-Identifier: LGPL-3.0

use crate::bindings;
use enum_map::{enum_map, Enum};
use std;
use std::borrow::Cow;
#[cfg(test)]
use std::collections;
use std::ffi::{CStr, OsStr, OsString};
use std::fmt::{self, Display};
use std::os::raw::c_void;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use std::vec::Vec;

use petgraph::prelude::NodeIndex;
use petgraph::visit::Dfs;
use petgraph::visit::IntoNodeReferences;

use enum_map::EnumMap;

use lazy_static::lazy_static;

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

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
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

const SHARED_PREFIX: &[u8] = b"shared:";

/// Converts `/home/symphorien/.cache/lorri/gc_roots/02ebed43adca1d7ca863ce9b0a537205/gc_root/shell_gc_root/` into `/home/symphorien/src/lorri/tests/integration/bug23_gopath/shell.nix`
fn resolve_lorri_root(path: &[u8]) -> std::io::Result<PathBuf> {
    let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path));
    let mut path = match path.parent() {
        Some(p) => p.to_owned(),
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "resolving / as lorri root",
            ))
        }
    };
    path.push("nix_file");
    std::fs::read_link(path)
}

/// A struct for human readable age of a link
///
/// displays as `, 3d ago` where units are d=day, m=month and y=year.
struct LinkAge(Option<SystemTime>);
impl Display for LinkAge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const DAY: Duration = Duration::from_secs(3600 * 24);
        const MONTH: Duration = Duration::from_secs(3600 * 24 * 30);
        const YEAR: Duration = Duration::from_secs(3600 * 24 * 365);
        if let Some(t) = &self.0 {
            if let Ok(duration) = t.elapsed() {
                return if duration < DAY {
                    write!(f, ", today")
                } else if duration < MONTH {
                    write!(f, ", {}d ago", duration.as_secs() / DAY.as_secs())
                } else if duration < YEAR {
                    write!(f, ", {}m ago", duration.as_secs() / MONTH.as_secs())
                } else {
                    write!(f, ", {}y ago", duration.as_secs() / YEAR.as_secs())
                };
            }
        }
        Ok(())
    }
}

impl NodeDescription {
    /// a short but human readable description of the node
    /// for a store path, only shows the name
    /// for a gc root, tells if it's a per-user profile, a NixOS generation, or a lorri gc
    /// does some amount of work, so you might want to cache it.
    pub fn name(&self) -> Cow<[u8]> {
        use self::NodeDescription::*;
        lazy_static! {
            static ref STORE_PATH: regex::bytes::Regex =
                regex::bytes::Regex::new(r"^/(?:.*)/[a-z0-9]*-([^/]*)$")
                    .expect("regex compilation failed");
            static ref PER_USER_PROFILE: regex::Regex =
                regex::Regex::new(r"^/(?:.*)/profiles/per-user/([^/]*)/([^/]*)-([0-9]*)-link$")
                    .expect("regex compilation failed");
            static ref SYSTEM_PROFILE: regex::Regex =
                regex::Regex::new(r"^/(?:.*)/profiles/system-([0-9]*)-link$")
                    .expect("regex compilation failed");
            static ref LORRI: regex::Regex = regex::Regex::new(
                r"^/home/([^/]*)/.cache/lorri/gc_roots/(?:[^/]*)/gc_root/shell_gc_root$"
            )
            .expect("regex compilation failed");
        };
        match self {
            Path(path) => match STORE_PATH.captures(&path) {
                Some(c) => {
                    let name = c.get(1).unwrap().as_bytes();
                    Cow::Borrowed(name)
                }
                None => Cow::Borrowed(&path),
            },
            Link(path) => match std::str::from_utf8(path) {
                Ok(path_str) => {
                    let link_age = match std::path::Path::new(std::ffi::OsStr::from_bytes(path))
                        .symlink_metadata()
                        .map(|m| m.modified())
                    {
                        Ok(Ok(time)) => LinkAge(Some(time)),
                        _ => LinkAge(None),
                    };
                    let fancy_desc = match PER_USER_PROFILE.captures(&path_str) {
                        Some(c) => {
                            let user = c.get(1).unwrap().as_str();
                            let profile = c.get(2).unwrap().as_str();
                            let gen = c.get(3).unwrap().as_str();
                            let desc = if profile == "profile" {
                                format!("generation {gen} of {user}'s profile{link_age}")
                            } else {
                                format!("generation {gen} of {user}'s profile {profile}{link_age}")
                            };
                            Some(desc)
                        }
                        None => match SYSTEM_PROFILE.captures(&path_str) {
                            Some(c) => {
                                let gen = c.get(1).unwrap().as_str();
                                let desc = format!("NixOS generation {gen}{link_age}");
                                Some(desc)
                            }
                            None => match LORRI.captures(&path_str) {
                                Some(c) => {
                                    let user = c.get(1).unwrap().as_str();
                                    match resolve_lorri_root(path) {
                                        Ok(nix_file) => {
                                            let desc = format!(
                                                "{user}'s lorri cache for {}{link_age}",
                                                nix_file.display()
                                            );
                                            Some(desc)
                                        }
                                        Err(_) => None,
                                    }
                                }
                                None => None,
                            },
                        },
                    };
                    match (&link_age, fancy_desc) {
                        (_, Some(f)) => Cow::Owned(f.into_bytes()),
                        (LinkAge(Some(_)), None) => {
                            Cow::Owned(format!("{path_str}{link_age}").into_bytes())
                        }
                        _ => Cow::Borrowed(path),
                    }
                }
                Err(_) => Cow::Borrowed(path),
            },
            Memory(path) | Temporary(path) => Cow::Borrowed(&path),
            Dummy => Cow::Borrowed(b"{dummy}"),
            FilteredOut => Cow::Borrowed(b"{filtered out}"),
            Transient => Cow::Borrowed(b"{transient}"),
            Shared(name) => {
                let mut res = Vec::with_capacity(SHARED_PREFIX.len() + name.len());
                res.extend(SHARED_PREFIX);
                res.extend(name);
                Cow::Owned(res)
            }
        }
    }

    /// returns the path as an `OsStr` if this node is on the filesystem
    pub fn path_as_os_str(&self) -> Option<&OsStr> {
        use self::NodeDescription::*;
        match self {
            Link(path) | Path(path) => Some(OsStr::from_bytes(path)),
            _ => None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        use self::NodeDescription::*;
        match self {
            Link(path) | Path(path) | Memory(path) | Temporary(path) => Some(&path),
            Shared(name) => Some(&name),
            Transient | Dummy | FilteredOut => None,
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

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct DepNode {
    pub description: NodeDescription,
    /// size in bytes
    pub size: u64,
}

impl DepNode {
    /// Note: clones the string describing the path.
    /// # Safety
    /// `p` must be a valid pointer and contain no null pointer members.
    /// Its `path` field must contain a valid C string.
    unsafe fn new(p: &bindings::path_t) -> Self {
        let path: Vec<u8> = CStr::from_ptr(p.path).to_bytes().to_vec();
        use self::NodeDescription::*;
        let description;
        if path[0] == b'/' {
            if path.starts_with(b"/proc/") {
                description = Memory(path);
            } else if p.is_root != 0 {
                description = Link(path);
            } else {
                description = Path(path);
            }
        } else if path.starts_with(b"{memory:") || path == b"{lsof}" || path == b"{censored}" {
            // {memory} is nix < 2.2 and was replaced by paths in /proc for linux and {lsof} for darwin in nix 2.3.
            // See https://github.com/NixOS/nix/commit/a3f37d87eabcfb5dc581abcfa46e5e7d387dfa8c
            // {censored} was introduced in nix 2.3:
            // https://github.com/NixOS/nix/commit/53522cb6ac19bd1da35a657988231cce9387be9c
            description = Memory(path);
        } else if path.starts_with(b"{temp:") {
            description = Temporary(path);
        } else {
            panic!(
                "Unknown store path type: {}",
                String::from_utf8_lossy(&path)
            );
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
        write!(f, "N({:?}, size={})", self.description, self.size)
    }
}

/// Whether all nodes are reachable from the root
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachability {
    Connected,
    Disconnected,
}

/// Whether deduplicated nodes are counted several times
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupAwareness {
    Aware,
    Unaware,
}

#[derive(Clone, Debug)]
pub struct SizeMetadata {
    pub reachable: Reachability,
    pub dedup: DedupAwareness,
    pub size: EnumMap<DedupAwareness, EnumMap<Reachability, Option<u64>>>,
}

pub type Edge = ();

pub type DepGraph = petgraph::graph::Graph<DepNode, Edge, petgraph::Directed>;

#[derive(Debug, Clone)]
pub struct DepInfos {
    pub graph: DepGraph,
    pub root: NodeIndex,
    pub metadata: SizeMetadata,
}

// symbol exported to libnix_adapter
/// # Safety
/// `g` must have been obtained by rust code, and not modified by C code.
/// `p` must be a valid pointer and contain no null pointer members.
/// Its `path` field must contain a valid C string.
#[no_mangle]
pub unsafe extern "C" fn register_node(g: *mut DepGraph, p: *const bindings::path_t) {
    let p: &bindings::path_t = p.as_ref().unwrap();
    let g: &mut DepGraph = g.as_mut().unwrap();
    let drv = DepNode::new(p);
    g.add_node(drv);
}

// symbol exported to libnix_adapter
/// # Safety
/// `g` must have been obtained by rust code, and not modified by C code.
#[no_mangle]
pub unsafe extern "C" fn register_edge(g: *mut DepGraph, from: u32, to: u32) {
    if from == to {
        return;
    }
    let g: &mut DepGraph = g.as_mut().unwrap();
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
            Some(path) => path.as_ptr(),
        };
        let res = unsafe { bindings::populateGraph(gptr, rootptr as *const std::os::raw::c_char) };

        if res != 0 {
            return Err(res);
        }
        let root_idx = match &root_data {
            None => g.add_node(DepNode::dummy()),
            Some(_) => NodeIndex::from(0),
        };
        let reachable = match &root_data {
            None => Reachability::Disconnected,
            Some(_) => Reachability::Connected,
        };
        let metadata = SizeMetadata {
            reachable,
            dedup: DedupAwareness::Unaware,
            size: enum_map! { _ => enum_map!{ _ => None }},
        };
        let mut di = DepInfos {
            root: root_idx,
            graph: g,
            metadata,
        };
        if root_data.is_none() {
            let gc_roots: Vec<_> = di
                .graph
                .node_references()
                .filter_map(|(idx, n)| {
                    if n.kind().is_gc_root() {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect();
            for root in gc_roots {
                di.graph.add_edge(di.root, root, ());
            }
        }
        di.record_metadata();
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

    /// returns the sum of the size of all the derivations
    pub fn size(&self) -> u64 {
        self.graph.raw_nodes().iter().map(|n| n.weight.size).sum()
    }

    /// records the current size of the graph in its metadata field.
    pub fn record_metadata(&mut self) {
        let dedup = self.metadata.dedup;
        macro_rules! entry {
            () => {
                self.metadata.size[dedup]
            };
        }
        if entry!()[Reachability::Connected].is_none() {
            entry!()[Reachability::Connected] = Some(self.reachable_size());
        }
        if self.metadata.reachable == Reachability::Disconnected
            && entry!()[Reachability::Disconnected].is_none()
        {
            entry!()[Reachability::Disconnected] = Some(self.size());
        }
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
            .map(|idx| {
                assert_ne!(idx, self.root);
                String::from_utf8_lossy(&self.graph[idx].name()).into()
            })
            .collect()
    }

    /// checks metadata is consistent
    #[cfg(test)]
    pub fn check_metadata(&self) {
        use self::Reachability::*;
        if self.metadata.reachable == Connected {
            let mut i = 0;
            let mut dfs = self.dfs();
            while let Some(_) = dfs.next(&self.graph) {
                i += 1;
            }
            assert_eq!(i, self.graph.node_count(), "metadata indicates graph is connected, but it is not");
        }
        let entry = &self.metadata.size[self.metadata.dedup];
        if let Some(s) = entry[self.metadata.reachable] {
            assert_eq!(s, self.size(), "wrong size in metadata");
        }
    }
}

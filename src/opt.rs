use crate::depgraph::*;
// use crate::msg::*;

use petgraph::prelude::NodeIndex;
use rayon::prelude::*;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Result;
use std::iter::once;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;
use walkdir::{DirEntryExt, WalkDir};

enum Owner {
    One(NodeIndex),
    Several(NodeIndex),
}


/// Stats all the files in the store looking for hardlinked files
/// and adapt the sizes of the nodes to take this into account.
pub fn refine_optimized_store(mut di: DepInfos) -> Result<DepInfos> {
    // invariant:
    // forall visited file:
    // its inode is a key in inode_to_owner
    // if this inode has been visited once, then the value is Owner::One(n)
    // where n is the NodeIndex of the derivation which lead to the file
    // if the inode has been visited more than once, then the value is
    // Owner::Several(n) where n is a node with the file's size and
    // forall store path containing this file, then there is an edge from the
    // corresponding node to this files's node.
    // In this case, parents do not count this file's size in their size.
    let mut inode_to_owner = BTreeMap::new();

    let indices = 0..di.graph.node_count();
    // let mut progress = Progress::new(indices.len());
    let graph_locked = Arc::new(RwLock::new(di));
    let iter = indices
        .into_par_iter()
        .flat_map_iter(|index: usize| -> Box<dyn Iterator<Item = _>> {
            // for (i, idx) in indices.drain(..).enumerate() {
            //     noisy!({
            //         progress.print(i);
            //     });
            let index = petgraph::graph::NodeIndex::new(index);

            let path: OsString;
            {
                // scope where we borrow the graph
                let weight = &graph_locked.read().expect("poisoned lock").graph[index];
                // roots are not necessary readable, and anyway they are symlinks
                if weight.kind() != NodeKind::Path {
                    return Box::new(std::iter::empty());
                }
                path = weight.description.path_as_os_str().unwrap().to_os_string();
            }

            // if path is a symlink to a directory, we enumerate files not in this
            // derivation.
            let p: &Path = path.as_ref();
            if p.symlink_metadata().unwrap().file_type().is_symlink() {
                return Box::new(std::iter::empty());
            };

            let walker = WalkDir::new(&path);
            Box::new(walker.into_iter().filter_map(move |entry| {
                entry
                    .map(|entry| {
                        // only files are hardlinked
                        if !entry.file_type().is_file() {
                            return None;
                        }
                        Some((index, entry.ino(), entry))
                    })
                    .transpose()
            }))
        });
    let (send, recv) = std::sync::mpsc::sync_channel(100);
    rayon::join(
        move || {
            iter.for_each(|el| {
                let _ = send.send(el);
            })
        },
        || {
            for entry in recv {
                let (index, inode, entry) = entry.unwrap();
                match inode_to_owner.entry(inode) {
                    Entry::Vacant(e) => {
                        // first time we see this inode
                        e.insert(Owner::One(index));
                    }
                    Entry::Occupied(mut e) => {
                        let mut graph = graph_locked.write().expect("poisoned lock");
                        // this inode is deduplicated
                        let metadata = entry.metadata().unwrap();
                        let v = e.get_mut();
                        let new_node = match *v {
                            Owner::One(n) => {
                                // second time we see this inode;
                                // let's create a "shared" node for these files
                                let name = graph.graph[index].name().into_owned();
                                let new_node = graph.graph.add_node(DepNode {
                                    description: NodeDescription::Shared(name),
                                    size: metadata.len(),
                                });
                                graph.graph.add_edge(n, new_node, ());
                                let new_w = &mut graph.graph[n];
                                new_w.size -= metadata.len();
                                *v = Owner::Several(new_node);
                                new_node
                            }
                            Owner::Several(n) => n,
                        };
                        graph.graph.add_edge(index, new_node, ());
                        let w = &mut graph.graph[index];
                        w.size -= metadata.len();
                    }
                }
            }
        },
    );
    di = Arc::try_unwrap(graph_locked).unwrap().into_inner().unwrap();
    di.metadata.dedup = DedupAwareness::Aware;
    di.record_metadata();
    Ok(di)
}

/// Determine whether at least one path has been optimised in the store.
/// This function is designed to be cheap, and to fail when it cannot be cheap
/// (it will return `Ok(None)` then).
pub fn store_is_optimised(di: &DepInfos) -> Result<Option<bool>> {
    // there is no way in the nix api to get the linksDir field of a RemoteStore
    // Using this api would only work for LocalStore, which is unfortunate.
    // So we just infer the linksDir from a drv. Not a gc root because it is
    // usually a symlink.
    let drv = match &di
        .graph
        .raw_nodes()
        .iter()
        .find(|node| node.weight.kind() == NodeKind::Path)
    {
        Some(ref node) => &node.weight,
        None => return Ok(None),
    };
    let mut p = PathBuf::from(drv.description.path_as_os_str().unwrap().to_os_string());
    // compute the location of .links
    if !p.pop() {
        return Ok(None);
    }
    p.push(".links");

    // iterate on the first ten files in .links, and then yield None and give up
    for entry in p.read_dir()?.map(Some).take(10).chain(once(None)) {
        let entry = match entry {
            Some(entry) => entry?,
            // .links contains more than ten files, give up
            None => return Ok(None),
        };
        let ty = entry.file_type()?;
        if !ty.is_file() {
            eprintln!("Strange, {} is not a file", entry.path().display());
            return Ok(None);
        }
        if entry.metadata()?.nlink() > 1 {
            // this file is optimised !
            return Ok(Some(true));
        }
    }
    Ok(Some(false))
}

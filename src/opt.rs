use crate::depgraph::*;
use crate::msg::*;

use dashmap::mapref::entry::Entry;
use petgraph::prelude::NodeIndex;
use rayon::prelude::*;
use std::io::Result;
use std::iter::once;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use walkdir::{DirEntryExt, WalkDir};

#[derive(Debug, Copy, Clone)]
enum Owner {
    One(NodeIndex),
    Several(NodeIndex),
}

/// Stats all the files in the store looking for hardlinked files
/// and adapt the sizes of the nodes to take this into account.
pub fn refine_optimized_store(di: &mut DepInfos) -> Result<()> {
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
    let inode_to_owner = dashmap::DashMap::new();

    let indices = 0..di.graph.node_count();
    let progress = if quiet() {
        indicatif::ProgressBar::hidden()
    } else {
        indicatif::ProgressBar::new(di.graph.node_count() as u64).with_style(
            indicatif::ProgressStyle::default_bar().template("{wide_bar} {percent}% ETA {eta}"),
        )
    };
    let locked_graph = Arc::new(RwLock::new(&mut di.graph));
    indices.into_par_iter().for_each(|i| {
        noisy!({
            progress.inc(1);
        });
        let idx = petgraph::graph::NodeIndex::new(i);

        let walker = {
            let graph = locked_graph.read().unwrap();
            // scope where we borrow the graph
            let weight = &graph[idx];
            // roots are not necessary readable, and anyway they are symlinks
            if weight.kind() != NodeKind::Path {
                return;
            }
            let path = std::path::Path::new(weight.description.path_as_os_str().unwrap());

            // if path is a symlink to a directory, we enumerate files not in this
            // derivation.
            if path.symlink_metadata().unwrap().file_type().is_symlink() {
                return;
            };

            WalkDir::new(&path)
        };
        for entry in walker {
            let entry = entry.unwrap();
            // only files are hardlinked
            if !entry.file_type().is_file() {
                continue;
            }
            let ino = entry.ino();
            // attempt to make the stat syscall without taking a write lock
            let must_stat = matches!(inode_to_owner.get(&ino).map(|x| *x), Some(Owner::One(_)));
            let filesize = if must_stat {
                Some(entry.metadata().unwrap().len())
            } else {
                None
            };

            match inode_to_owner.entry(ino) {
                Entry::Vacant(e) => {
                    // first time we see this inode
                    e.insert(Owner::One(idx));
                }
                Entry::Occupied(mut e) => {
                    // this inode is deduplicated
                    let v = e.get_mut();
                    let (new_node, mut graph) = match *v {
                        Owner::One(n) => {
                            // second time we see this inode;
                            // let's create a "shared" node for these files
                            let filesize =
                                filesize.unwrap_or_else(|| entry.metadata().unwrap().len());
                            let mut graph = locked_graph.write().unwrap();
                            let name = graph[idx].name().into_owned();
                            let new_node = graph.add_node(DepNode {
                                description: NodeDescription::Shared(name),
                                size: filesize,
                            });
                            graph.add_edge(n, new_node, ());
                            let new_w = &mut graph[n];
                            new_w.size -= filesize;
                            *v = Owner::Several(new_node);
                            (new_node, graph)
                        }
                        Owner::Several(n) => (n, locked_graph.write().unwrap()),
                    };
                    graph.add_edge(idx, new_node, ());
                    let filesize = graph[new_node].size;
                    let w = &mut graph[idx];
                    w.size -= filesize;
                }
            }
        }
    });
    di.metadata.dedup = DedupAwareness::Aware;
    progress.finish_and_clear();
    di.record_metadata();
    Ok(())
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

extern crate walkdir;

use depgraph::*;
use msg::*;

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::io::Result;
use std::cell::Cell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::iter::once;
use std::os::unix::fs::MetadataExt;
use self::walkdir::{WalkDir, DirEntryExt};
use petgraph::prelude::NodeIndex;

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
    let mut inode_to_owner = BTreeMap::new();

    let mut indices: Vec<NodeIndex> = di.graph.node_indices().collect();
    let mut progress = Progress::new(indices.len());
    for (i, idx) in indices.drain(..).enumerate() {
        noisy!({
            progress.print(i);
        });

        let path: OsString;
        {
            // scope where we borrow the graph
            let weight = &di.graph[idx];
            // roots are not necessary readable, and anyway they are symlinks
            if weight.kind() != NodeKind::Path {
                continue;
            }
            path = weight.description.path_as_os_str().unwrap().to_os_string();
        }

        // if path is a symlink to a directory, we enumerate files not in this
        // derivation.
        let p: &Path = path.as_ref();
        if p.symlink_metadata()?.file_type().is_symlink() {
            continue;
        };

        let mut walker = WalkDir::new(&path);
        for entry in walker {
            let entry = entry?;
            // only files are hardlinked
            if !entry.file_type().is_file() {
                continue;
            }
            let ino = entry.ino();
            match inode_to_owner.entry(ino) {
                Entry::Vacant(mut e) => {
                    // first time we see this inode
                    e.insert(Owner::One(idx));
                }
                Entry::Occupied(mut e) => {
                    // this inode is deduplicated
                    let metadata = entry.metadata()?;
                    let v = e.get_mut();
                    let new_node = match *v {
                        Owner::One(n) => {
                            // second time we see this inode;
                            // let's create a "shared" node for these files
                            let name = di.graph[idx].name().into_owned();
                            let new_node = di.graph.add_node(DepNode {
                                description: NodeDescription::Shared(name),
                                size: Cell::new(metadata.len()),
                            });
                            di.graph.add_edge(n, new_node, ());
                            let new_w = &di.graph[n];
                            new_w.size.set(new_w.size.get() - metadata.len());
                            *v = Owner::Several(new_node);
                            new_node
                        }
                        Owner::Several(n) => n,
                    };
                    di.graph.add_edge(idx, new_node, ());
                    let w = &di.graph[idx];
                    w.size.set(w.size.get() - metadata.len());
                }
            }
        }
    }
    di.metadata.dedup = DedupAwareness::Aware;
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
    let drv = match &di.graph.raw_nodes().iter().find(
        |node| node.weight.kind() == NodeKind::Path,
    ) {
        &Some(ref node) => &node.weight,
        &None => return Ok(None),
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
    return Ok(Some(false));
}

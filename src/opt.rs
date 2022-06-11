use crate::depgraph::*;
// use crate::msg::*;

use petgraph::prelude::NodeIndex;
use rayon::prelude::*;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::Result;
use std::iter::once;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
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
    let mut inode_to_owner = HashMap::new();

    let paths: Vec<(petgraph::graph::NodeIndex, PathBuf)> = di.graph.raw_nodes().par_iter().enumerate().filter_map(|(i, r)| {
        // roots are not necessary readable, and anyway they are symlinks
        if r.weight.kind() != NodeKind::Path {
            return None
        }
        let path = std::path::Path::new(r.weight.description.path_as_os_str().unwrap());
        // if path is a symlink to a directory, we would enumerate files not in this
        // derivation.
        if path.symlink_metadata().unwrap().file_type().is_symlink() {
            return None
        }
        Some((petgraph::graph::NodeIndex::new(i), path.to_path_buf()))
    }).collect();

    let iter = paths
        .into_par_iter()
        .flat_map_iter(|(index, path)| {
            let walker = WalkDir::new(&path);
            walker.into_iter().filter_map(move |entry| {
                entry
                    .map(|entry| {
                        // only files are hardlinked
                        if !entry.file_type().is_file() {
                            return None;
                        }
                        let metadata = entry.metadata().unwrap();
                        Some((index, entry.ino(), metadata.len()))
                    })
                    .transpose()
            })
        });
    let (send, recv) = crossbeam_channel::unbounded();
    rayon::join(
        move || {
            iter.for_each(|el| {
                let _ = send.send(el).unwrap();
            })
        },
        || {
            let mut n = 0;
            while let Ok(entry) = recv.recv() {
            // for entry in recv {
                let (index, inode, filesize) = entry.unwrap();
                n+=1;
                if n % 100_000 == 0 {
                    dbg!(recv.len());
                }
                match inode_to_owner.entry(inode) {
                    Entry::Vacant(e) => {
                        // first time we see this inode
                        e.insert(Owner::One(index));
                    }
                    Entry::Occupied(mut e) => {
                        // this inode is deduplicated
                        let v = e.get_mut();
                        let new_node = match *v {
                            Owner::One(n) => {
                                // second time we see this inode;
                                // let's create a "shared" node for these files
                                let name = di.graph[index].name().into_owned();
                                let new_node = di.graph.add_node(DepNode {
                                    description: NodeDescription::Shared(name),
                                    size: filesize,
                                });
                                di.graph.add_edge(n, new_node, ());
                                let new_w = &mut di.graph[n];
                                new_w.size -= filesize;
                                *v = Owner::Several(new_node);
                                new_node
                            }
                            Owner::Several(n) => n,
                        };
                        di.graph.add_edge(index, new_node, ());
                        let w = &mut di.graph[index];
                        w.size -= filesize;
                    }
                }
            }
        },
    );
    dbg!(inode_to_owner.values().filter(|x| matches!(x, Owner::One(_))).count());
    dbg!(inode_to_owner.len());
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

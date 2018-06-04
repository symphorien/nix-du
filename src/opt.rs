extern crate walkdir;

use depgraph::*;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::io::Result;
use std::ffi::CString;
use std::path::Path;
use self::walkdir::{WalkDir, DirEntryExt};
use petgraph::prelude::NodeIndex;


enum Owner {
    One(NodeIndex),
    Several(NodeIndex),
}

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
    let total = indices.len();
    for (i, idx) in indices.drain(..).enumerate() {
        // FIXME: progress
        if i % 1000 == 0 {
            eprint!("{} sur {}\r", i, total);
        }

        let owned_path: String;
        {
            // scope where we borrow the graph
            let weight = &di.graph[idx];
            // roots are not necessary readable, and anyway thery are symlinks
            // we also filter out dummy nodes like {memory}
            if weight.is_root || weight.path.to_bytes().get(0) != Some(&b'/') {
                continue;
            }
            owned_path = weight.path.to_string_lossy().into_owned(); // FIXME lossy
        }
        let path = &Path::new(&owned_path);

        // if path is a symlink to a directory, we enumerate files not in this
        // derivation.
        if path.symlink_metadata()?.file_type().is_symlink() {
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
                    e.insert(Owner::One(idx));
                }
                Entry::Occupied(mut e) => {
                    let metadata = entry.metadata()?;
                    let v = e.get_mut();
                    let new_node = match *v {
                        Owner::One(n) => {
                            let new_node = di.graph.add_node(Derivation {
                                path: CString::new(format!("shared under {}", owned_path)).unwrap(),
                                size: metadata.len(),
                                is_root: false,
                            });
                            di.graph.add_edge(n, new_node, ());
                            di.graph[n].size -= metadata.len();
                            *v = Owner::Several(n);
                            new_node
                        }
                        Owner::Several(n) => n,
                    };
                    di.graph.add_edge(idx, new_node, ());
                    di.graph[idx].size -= metadata.len();
                }
            }
        }
    }
    Ok(())
}

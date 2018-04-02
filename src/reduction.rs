// SPDX-License-Identifier: LGPL-3.0

extern crate fixedbitset;
extern crate memchr;
extern crate petgraph;

use std;
use std::collections;

use petgraph::visit::EdgeRef;

use depgraph::*;

/// Merges all the in memory roots in one root.
pub fn merge_inmemory_roots(di: DepInfos) -> DepInfos {
    let DepInfos {
        mut roots,
        mut graph,
    } = di;
    let fake_root = Derivation {
        path: std::ffi::CString::new("{memory}").unwrap(),
        size: 0,
        is_root: true,
    };
    let fake_root_idx = graph.add_node(fake_root);

    roots = roots
        .iter()
        .cloned()
        .filter(|&idx| if graph[idx].is_inmemory_root() {
            graph.add_edge(fake_root_idx, idx, ());
            graph[idx].is_root = false;
            false
        } else {
            true
        })
        .collect();

    roots.push(fake_root_idx);

    DepInfos { roots, graph }
}



/// Computes a sort of condensation of the graph.
///
/// Precisely, let `roots(v)` be the set of roots depending transitively on a vertex `v`.
/// Let the input graph be `G=(V, E)`. This function returns the graph
/// `(V', E')` where `V'` is the quotient of `V` by the equivalence relation
/// "two vertices are equivalent if they have the same image by `roots`"
/// and and edge is in `E'` if there are vertices in the source and target
/// equivalence class which have a corresponding edge in `G`.
///
/// Complexity: with n vertices, m edges and r roots:
/// * nln(r)+m in space
/// * nln(n)+m in time
///
/// Expected simplification: as I write theses lines, on my store (`NixOS`, 37G)
/// * before: n=37594, m=262914
/// * after `condense`: n=61, m=211
pub fn condense(mut di: DepInfos) -> DepInfos {
    let template = fixedbitset::FixedBitSet::with_capacity(di.roots.len());
    let mut g = di.graph.map(|_, _| template.clone(), |_, _| ());

    // add a fake root
    let fake_root = g.add_node(template);
    for root in &di.roots {
        g.add_edge(fake_root, *root, ());
    }

    // label each node with roots it is a dependence of
    for (i, root) in (&di.roots).iter().cloned().enumerate() {
        let mut bfs = petgraph::visit::Bfs::new(&g, root);
        while let Some(nx) = bfs.next(&g) {
            g[nx].insert(i);
        }
    }

    let mut bfs = petgraph::visit::Bfs::new(&g, fake_root);
    let _ = bfs.next(&g); // skip the fake root

    // now remove spurious elements from the original graph.
    // removing nodes is slow, so we create a new graph for that.
    let mut new_ids = collections::BTreeMap::new(); // set of roots => new node index
    let mut new_graph = DepGraph::new();

    // we take as representative the topmost element of the class,
    // topmost as in depth -- the first reached in a BFS
    while let Some(idx) = bfs.next(&g) {
        if idx >= fake_root {
            continue;
        }
        let representative = &g[idx];
        let new_node = new_ids.entry(representative).or_insert_with(|| {
            let mut w = Derivation::dummy();
            std::mem::swap(&mut w, &mut di.graph[idx]);
            new_graph.add_node(w)
        });
        new_graph[*new_node].size += di.graph[idx].size;
    }

    // keep edges
    for edge in g.raw_edges() {
        let from = &g[edge.source()];
        let to = &g[edge.target()];
        if from != to {
            // unreachable nodes don't have a counterpart in the new graph
            if let (Some(&newfrom), Some(&newto)) = (new_ids.get(&from), new_ids.get(&to)) {
                new_graph.update_edge(newfrom, newto, ());
            }
        }
    }
    DepInfos::new_from_graph(new_graph)
}

/// Creates a new graph only retaining roots and nodes whose weight return
/// `true` when passed to `filter`. The nodes which are dropped are
/// merged into an arbitrary parent (ie. the name is dropped, but edges and size
/// are merged).
///
/// Note that `filter` will be called at most once per node.
pub fn keep<T: Fn(&Derivation) -> bool>(mut di: DepInfos, filter: T) -> DepInfos {
    let mut new_ids = collections::BTreeMap::new();
    let mut new_graph = DepGraph::new();

    for idx in di.graph.node_indices() {
        if di.graph[idx].is_root || filter(&di.graph[idx]) {
            let mut new_w = Derivation::dummy();
            std::mem::swap(&mut di.graph[idx], &mut new_w);
            new_ids.insert(idx, new_graph.add_node(new_w));
        }
    }
    let frozen = petgraph::graph::Frozen::new(&mut di.graph);
    for (&old, &new) in &new_ids {
        let filtered = petgraph::visit::EdgeFiltered::from_fn(&*frozen, |e| {
            e.source() == old || !new_ids.contains_key(&e.source())
        });
        let mut dfs = petgraph::visit::Dfs::new(&filtered, old);
        let old_ = dfs.next(&filtered); // skip old
        debug_assert_eq!(Some(old), old_);
        while let Some(idx) = dfs.next(&filtered) {
            if let Some(&new2) = new_ids.get(&idx) {
                new_graph.add_edge(new, new2, ());
            } else {
                new_graph[new].size += frozen[idx].size;
                unsafe {
                    let w: *mut Derivation = &frozen[idx] as *const _ as *mut _;
                    (*w).size = 0;
                }
            }
        }
    }
    DepInfos::new_from_graph(new_graph)
}

#[cfg(test)]
mod tests {
    extern crate petgraph;
    extern crate rand;
    use self::rand::distributions::{IndependentSample, Weighted, WeightedChoice};
    use self::rand::Rng;
    use depgraph::*;
    use reduction::*;
    use std::collections;
    use std::ffi::CString;
    use petgraph::prelude::NodeIndex;
    use petgraph::visit::IntoNodeReferences;

    /// asserts that `transform` preserves
    /// * the set of roots, py path
    /// * reachable size
    /// and returns a coherent `DepInfos` (as per `roots_attr_coherent`)
    fn check_invariants<T: Fn(DepInfos) -> DepInfos>(transform: T, di: DepInfos, same_roots: bool) {
        let orig = di.clone();
        let new = transform(di);
        if same_roots {
            assert_eq!(new.roots_name(), orig.roots_name());
        }
        assert_eq!(new.reachable_size(), orig.reachable_size());
        assert!(new.roots_attr_coherent());
    }
    /// generates a random `DepInfos` where
    /// * all derivations have a distinct path
    /// * there are `size` derivations
    /// * the expected average degree of the graph should be `avg_degree`
    /// * the first 62 nodes have size `1<<index`
    fn generate_random(size: u32, avg_degree: u32) -> DepInfos {
        assert!(avg_degree <= size - 1);
        let mut items = vec![
            Weighted {
                weight: avg_degree,
                item: true,
            },
            Weighted {
                weight: size - 1 - avg_degree,
                item: false,
            },
        ];
        let wc = WeightedChoice::new(&mut items);
        let mut rng = rand::thread_rng();
        let mut g: DepGraph = petgraph::graph::Graph::new();
        for i in 0..size {
            let name = if rng.gen() {
                i.to_string()
            } else {
                format!("{{memory:{}}}", i)
            };
            let path = CString::new(name).unwrap();
            let size = if i < 62 {
                1u64 << i
            } else {
                3 + 2 * (i as u64)
            };
            let w = Derivation {
                is_root: false,
                path,
                size,
            };
            g.add_node(w);
        }
        for i in 0..size {
            for j in (i + 1)..size {
                if wc.ind_sample(&mut rng) {
                    g.add_edge(NodeIndex::from(i), NodeIndex::from(j), ());
                }
            }
        }
        let roots: std::vec::Vec<NodeIndex> = g.externals(petgraph::Direction::Incoming).collect();
        for &idx in &roots {
            g[idx].is_root = true;
        }
        let di = DepInfos { graph: g, roots };
        assert!(di.roots_attr_coherent());
        di
    }
    fn size_to_old_nodes(drv: &Derivation) -> collections::BTreeSet<NodeIndex> {
        (0..62)
            .filter(|i| drv.size & (1u64 << i) != 0)
            .map(NodeIndex::from)
            .collect()
    }
    fn path_to_old_size(drv: &Derivation) -> u32 {
        let only_digits: Vec<u8> = drv.path
            .as_bytes()
            .iter()
            .cloned()
            .filter(|x| x.is_ascii_digit())
            .collect();
        match String::from_utf8_lossy(&only_digits).parse() {
            Ok(x) => x,
            Err(_) => panic!("Cannot convert {:?} {:?}", drv.path, only_digits),
        }
    }
    #[test]
    /// check that condense and keep preserve some invariants
    fn invariants() {
        for _ in 0..40 {
            let di = generate_random(250, 10);
            check_invariants(merge_inmemory_roots, di.clone(), false);
            check_invariants(condense, di.clone(), true);
            check_invariants(|x| keep(x, |_| false), di.clone(), true);
            check_invariants(|x| keep(x, |_| true), di.clone(), true);
        }
    }
    #[test]
    fn check_merge_inmemory_roots() {
        for _ in 0..40 {
            let old = generate_random(250, 10);
            let new = merge_inmemory_roots(old.clone());
            for edge in new.graph.edge_references() {
                let old_child = &old.graph[edge.target()];
                let new_child = &new.graph[edge.target()];
                let new_parent = &new.graph[edge.source()];
                if old.graph.edge_weight(edge.id()).is_some() {
                    let old_parent = &old.graph[edge.source()];
                    assert_eq!(old_parent.path, new_parent.path);
                    assert_eq!(old_parent.size, new_parent.size);
                    assert_eq!(old_child, new_child);
                    if old_parent.is_root != new_parent.is_root {
                        assert!(old_parent.is_root);
                        assert!(!new_parent.is_root);
                    }
                } else {
                    assert!(old_child.is_inmemory_root());
                    assert!(old_child.is_root);
                    assert!(!new_child.is_root);
                    assert_eq!(new_parent.path.as_bytes(), b"{memory}");
                    assert_eq!(new_parent.size, 0);
                    assert_eq!(new_parent.is_root, true);
                }
            }
        }
    }

    #[test]
    fn check_condense() {
        // 62 so that each node is uniquely determined by its size, and
        // merging nodes doesn't destroy this information
        for _ in 0..80 {
            let old = generate_random(62, 10);
            let mut old_rev = old.graph.clone();
            old_rev.reverse();
            let new = condense(old.clone());
            let mut new_rev = new.graph.clone();
            new_rev.reverse();
            let oldroots: collections::BTreeSet<NodeIndex> = old.roots.iter().cloned().collect();
            let get_dependent_roots = |which, idx| {
                let grev = if which { &new_rev } else { &old_rev };
                let mut dfs = petgraph::visit::Dfs::new(grev, idx);
                let mut res = collections::BTreeSet::new();
                while let Some(nx) = dfs.next(grev) {
                    if grev[nx].is_root {
                        res.extend(&size_to_old_nodes(&grev[nx]) & &oldroots);
                    }
                }
                res
            };
            let mut nodes_image = collections::BTreeSet::<collections::BTreeSet<_>>::new();
            for (idx, drv) in new.graph.node_references() {
                let after = get_dependent_roots(true, idx);
                let elements = size_to_old_nodes(drv);
                for &element in &elements {
                    let before = get_dependent_roots(false, element);
                    assert_eq!(
                        before,
                        after,
                        "new:{:?} and old:{:?} do not belong to the same equivalence class ({:?} != {:?})",
                        idx,
                        element,
                        after,
                        before
                    );
                }
                nodes_image.insert(after);
                // here check edges
                for (idx2, drv2) in new.graph.node_references() {
                    let targets = size_to_old_nodes(drv2);
                    let should_exist = idx != idx2 &&
                        elements.iter().any(|&from| {
                            targets.iter().any(
                                |&to| old.graph.find_edge(from, to).is_some(),
                            )
                        });
                    let exists = new.graph.find_edge(idx, idx2).is_some();
                    assert_eq!(
                        should_exist,
                        exists,
                        "edge {:?} -> {:?} is wrong (expected: {:?})",
                        idx,
                        idx2,
                        should_exist
                    );
                }

            }
            assert_eq!(
                nodes_image.len(),
                new.graph.node_count(),
                "two nodes at least have the same equivalence class"
            );
        }
    }
    #[test]
    fn check_keep() {
        let filter_drv = |drv: &Derivation| drv.size % 3 == 2; // half of the drvs
        let real_filter = |drv: &Derivation| drv.is_root || filter_drv(drv);
        for _ in 0..50 {
            let old = generate_random(62, 10);
            let new = keep(old.clone(), &filter_drv);
            println!(
                "OLD:\n{:?}\nNew:\n{:?}",
                petgraph::dot::Dot::new(&old.graph),
                petgraph::dot::Dot::new(&new.graph)
            );
            // nodes:
            //   * labels
            let labels = |di: &DepInfos, all| {
                di.graph
                    .raw_nodes()
                    .iter()
                    .filter_map(|n| if all || real_filter(&n.weight) {
                        Some(n.weight.path.clone())
                    } else {
                        None
                    })
                    .collect::<collections::BTreeSet<_>>()
            };
            assert_eq!(labels(&old, false), labels(&new, true));
            //  * size
            let filtered = petgraph::visit::EdgeFiltered::from_fn(
                &old.graph,
                |e| !filter_drv(&old.graph[e.target()]),
            );
            let filtered2 = petgraph::visit::EdgeFiltered::from_fn(
                &old.graph,
                |e| !filter_drv(&old.graph[e.source()]),
            );
            let mut space = petgraph::algo::DfsSpace::new(&filtered);
            for (id, drv) in new.graph.node_references() {
                let top = NodeIndex::from(path_to_old_size(drv));
                assert!(drv.size & (1u64 << top.index()) != 0);
                for child in size_to_old_nodes(drv) {
                    assert!(
                        petgraph::algo::has_path_connecting(&filtered, top, child, Some(&mut space)),
                        "should not have coalesced {:?} and {:?}",
                        top,
                        child
                    );
                }
                // also check edges from here
                for (id2, drv2) in new.graph.node_references() {
                    let bottom = NodeIndex::from(path_to_old_size(drv2));
                    let targets = size_to_old_nodes(drv2);
                    let mut path_from_here_to = |targets: collections::BTreeSet<NodeIndex>| {
                        targets.iter().any(|&target| {
                            old.graph.find_edge(top, target).is_some() ||
                                old.graph.edges(top).any(|edge| {
                                    let intermediate = edge.target();
                                    petgraph::algo::has_path_connecting(
                                        &filtered2,
                                        intermediate,
                                        target,
                                        Some(&mut space),
                                    )
                                })
                        })
                    };
                    let should_exist = id != id2 &&
                        path_from_here_to([bottom].iter().cloned().collect());
                    let may_exist = id != id2 && path_from_here_to(targets);
                    let exists = new.graph.find_edge(id, id2).is_some();
                    // should => exists /\ exists => may
                    assert!(
                        (!should_exist || exists) && (!exists || may_exist),
                        "edge {:?} -> {:?} is debatable (expected: {:?}, acceptable: {:?})",
                        id,
                        id2,
                        should_exist,
                        may_exist
                    );
                }
            }
        }
    }
}

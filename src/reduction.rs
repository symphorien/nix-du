// SPDX-License-Identifier: LGPL-3.0

extern crate memchr;
extern crate petgraph;

use std;
use std::collections;

use petgraph::prelude::NodeIndex;
use petgraph::visit::EdgeRef;

use depgraph::*;

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
/// * n²+m in space
/// * (n²+m)r in time
///
/// Expected simplification: as I write theses lines, on my store (`NixOS`, 37G)
/// * before: n=37594, m=262914
/// * after `condense`: n=61, m=211
pub fn condense(mut di: DepInfos) -> DepInfos {
    let mut g = di.graph.map(|_, _| 0u16, |_, _| ());

    // add a fake root
    let fake_root = g.add_node(0);
    for root in &di.roots {
        g.add_edge(fake_root, *root, ());
    }

    // label each node with its "rsize", the number of roots it is a dependence of
    let mut max_rsize = 0;
    for root in (&di.roots).iter().cloned() {
        let mut bfs = petgraph::visit::Bfs::new(&g, root);
        while let Some(nx) = bfs.next(&g) {
            g[nx] += 1;
            max_rsize = std::cmp::max(max_rsize, g[nx]);
        }
    }

    // for each pair of nodes with same rsize, to know whether they are in the same
    // class, we add a child node.
    max_rsize += 1;
    let mut nodes_by_rsize = std::iter::repeat(Vec::new())
        .take(max_rsize as usize)
        .collect::<Vec<Vec<NodeIndex>>>();
    for idx in g.node_indices() {
        let rsize = g[idx] as usize;
        if rsize > 0 {
            nodes_by_rsize[rsize].push(idx);
        }
        g[idx] = 0;
    }
    for n in &nodes_by_rsize {
        for &i in n {
            for &j in n {
                if i != j {
                    let x = g.add_node(0);
                    g.add_edge(i, x, ());
                    g.add_edge(j, x, ());
                }
            }
        }
    }

    // label each node with its "rsize", the number of roots it is a dependence of
    for root in (&di.roots).iter().cloned() {
        let mut bfs = petgraph::visit::Bfs::new(&g, root);
        while let Some(nx) = bfs.next(&g) {
            g[nx] += 1;
        }
    }

    // compute equivalence classes
    let mut uf = petgraph::unionfind::UnionFind::new(g.node_count());
    for edge in g.raw_edges() {
        // parent and child are in the same class iff they have the same label
        if g[edge.source()] == g[edge.target()] {
            uf.union(edge.source().index(), edge.target().index());
        }
    }

    let mut bfs = petgraph::visit::Bfs::new(&g, fake_root);
    let _ = bfs.next(&g); // skip the fake root

    // now remove spurious elements from the original graph.
    // removing nodes is slow, so we create a new graph for that.
    let mut new_ids = collections::BTreeMap::new();
    let mut new_graph = DepGraph::new();

    // we take as representative the topmost element of the class,
    // topmost as in depth -- the first reached in a BFS
    while let Some(idx) = bfs.next(&g) {
        if idx >= fake_root {
            continue;
        }
        let representative = NodeIndex::from(uf.find_mut(idx.index()));
        let new_node = new_ids.entry(representative).or_insert_with(|| {
            let mut w = Derivation::dummy();
            std::mem::swap(&mut w, &mut di.graph[idx]);
            new_graph.add_node(w)
        });
        new_graph[*new_node].size += di.graph[idx].size;
    }

    // keep edges
    for edge in g.raw_edges() {
        let from = NodeIndex::from(uf.find(edge.source().index()));
        let to = NodeIndex::from(uf.find(edge.target().index()));
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
pub fn keep(mut di: DepInfos, filter: &Fn(&Derivation) -> bool) -> DepInfos {
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
    fn check_invariants<T: Fn(DepInfos) -> DepInfos>(transform: T, di: DepInfos) {
        let orig = di.clone();
        let new = transform(di);
        assert_eq!(new.roots_name(), orig.roots_name());
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
            let path = CString::new(i.to_string()).unwrap();
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
    #[test]
    /// check that condense and keep preserve some invariants
    fn invariants() {
        for _ in 0..40 {
            let di = generate_random(250, 10);
            check_invariants(condense, di.clone());
            check_invariants(|x| keep(x, &|_| false), di.clone());
            check_invariants(|x| keep(x, &|_| true), di.clone());
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
                        "new:{:?} and old:{:?} do not belong to the same equlivalence class ({:?} != {:?})",
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
                let top = NodeIndex::from(drv.path.to_str().unwrap().parse::<u32>().unwrap());
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
                    let bottom =
                        NodeIndex::from(drv2.path.to_str().unwrap().parse::<u32>().unwrap());
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

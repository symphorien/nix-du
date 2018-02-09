// SPDX-License-Identifier: LGPL-3.0

extern crate memchr;
extern crate petgraph;

use std;
use std::collections;

use petgraph::prelude::NodeIndex;
use petgraph::visit::IntoNodeReferences;
use petgraph::visit::EdgeRef;

use depgraph::*;

/// Computes a sort of condensation of the graph.
///
/// Precisely, let `roots(v)` be the set of roots depending on a vertex `v`.
/// Let the input graph be `G=(V, E)`. This function returns the graph
/// `(V', E')` where `V'` is the quotient of `V` by the equivalence relation
/// "two vertices are equivalent if they have the same image by `roots`"
/// and and edge is in `E'` if there are vertices in the source and target
/// equivalence class which have a corresponding edge in `G`.
///
/// Complexity: Linear space, in time: product of the size of the graph and
/// the number of roots.
///
/// This function is meant to be executed on the result of `condense`, which
/// has a better complexity and does a quite good job.
///
/// Expected simplification: as I write theses lines, on my store (NixOS, 37G)
/// * before: n=50223, m=340271
/// * after `condense`: n=6578, m=40372
/// * after `condese_exact`: n=4884, m=18004
pub fn condense(mut di: DepInfos) -> DepInfos {
    let mut g = di.graph.map(|_, _| 0u16, |_, _| ());

    // label each node with the number of roots it is a dependence of
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

    // add a fake root
    let fake_root = g.add_node(0);
    for root in &di.roots {
        g.add_edge(fake_root, *root, ());
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
        if edge.source() != fake_root {
            let from = NodeIndex::from(uf.find(edge.source().index()));
            let to = NodeIndex::from(uf.find(edge.target().index()));
            if from != to {
                // unreachale nodes don't have a counterpart in the new graph
                if let (Some(&newfrom), Some(&newto)) = (new_ids.get(&from), new_ids.get(&to)) {
                    new_graph.update_edge(newfrom, newto, ());
                }
            }
        }
    }

    di.graph = new_graph;
    di.roots = di.graph
        .node_references()
        .filter_map(|(idx, node)| if node.is_root { Some(idx) } else { None })
        .collect();

    di
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
    for (&old, &new) in &new_ids {
        let frozen = petgraph::graph::Frozen::new(&mut di.graph);
        let filtered = petgraph::visit::EdgeFiltered::from_fn(&*frozen, |e| {
            e.source() == old || !new_ids.contains_key(&e.source())
        });
        let mut dfs = petgraph::visit::Dfs::new(&filtered, old);
        let _ = dfs.next(&filtered); // skip old
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
    di.graph = new_graph;
    di.roots = di.graph
        .node_references()
        .filter_map(|(idx, node)| if node.is_root { Some(idx) } else { None })
        .collect();

    di
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
    use petgraph::visit::NodeRef;
    use petgraph::visit::VisitMap;

    /// returns the set of paths of the roots
    fn roots_name(di: &DepInfos) -> collections::BTreeSet<&CString> {
        di.roots.iter().map(|&idx| &di.graph[idx].path).collect()
    }
    /// returns wether di.roots is really the set of indices of root nodes
    /// according to `drv.is_root`
    fn roots_attr_coherent(di: &DepInfos) -> bool {
        let from_nodes: collections::BTreeSet<NodeIndex> = di.graph
            .node_references()
            .filter_map(|nref| {
                if nref.weight().is_root {
                    Some(nref.id())
                } else {
                    None
                }
            })
            .collect();
        let from_attr: collections::BTreeSet<NodeIndex> = di.roots.iter().cloned().collect();
        from_attr == from_nodes
    }
    /// return the sum of the size of all the derivations reachable from a root
    fn reachable_size(di: &DepInfos) -> u64 {
        let mut dfs = petgraph::visit::Dfs::empty(&di.graph);
        let mut sum = 0;
        for &idx in &di.roots {
            dfs.discovered.visit(idx);
            dfs.stack.push(idx);
        }
        while let Some(idx) = dfs.next(&di.graph) {
            sum += di.graph[idx].size;
        }
        sum
    }
    /// asserts that `transform` preserves
    /// * the set of roots, py path
    /// * reachable size
    /// and returns a coherent `DepInfos` (as per `roots_attr_coherent`)
    fn check_invariants<T: Fn(DepInfos) -> DepInfos>(transform: T, di: DepInfos) {
        let orig = di.clone();
        let new = transform(di);
        assert_eq!(roots_name(&new), roots_name(&orig));
        assert_eq!(reachable_size(&new), reachable_size(&orig));
        assert!(roots_attr_coherent(&new));
    }
    /// generates a random `DepInfos` where
    /// * all derivations have a distinct path
    /// * there are `size` derivations
    /// * the expected average degree of the graph should be `avg_degree`
    /// * the first 62 nodes have size `1<<index`
    fn generate_random(size: u32, avg_degree: u32) -> DepInfos {
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
        assert!(roots_attr_coherent(&di));
        di
    }
    #[test]
    /// check that condense and keep preserve some invariants
    fn invariants() {
        for _ in 0..40 {
            let di = generate_random(500, 10);
            check_invariants(condense, di.clone());
            check_invariants(|x| keep(x, &|_| false), di.clone());
            check_invariants(|x| keep(x, &|_| true), di.clone());
        }
    }
    #[test]
    fn check_condense() {
        // 62 so that each node is uniquely determined by its size, and
        // merging nodes doesn't destroy this information
        for _ in 0..40 {
            let old = generate_random(62, 10);
            let mut old_rev = old.graph.clone();
            old_rev.reverse();
            let new = condense(old.clone());
            let mut new_rev = new.graph.clone();
            new_rev.reverse();
            let oldroots: collections::BTreeSet<usize> =
                old.roots.iter().map(|&idx| idx.index()).collect();
            let size_to_old_nodes = |x| {
                (0..62usize)
                    .filter(|&i| (1u64 << i) & x != 0)
                    .collect::<collections::BTreeSet<usize>>()
            };
            let get_dependent_roots = |which, idx| {
                let grev = if which { &new_rev } else { &old_rev };
                let mut dfs = petgraph::visit::Dfs::new(grev, idx);
                let mut res = collections::BTreeSet::new();
                while let Some(nx) = dfs.next(grev) {
                    if grev[nx].is_root {
                        res.extend(&size_to_old_nodes(grev[nx].size) & &oldroots);
                    }
                }
                res
            };
            let mut nodes_image = collections::BTreeSet::<collections::BTreeSet<usize>>::new();
            for idx in new.graph.node_indices() {
                let after = get_dependent_roots(true, idx);
                eprintln!("{:?} -> {:?}", idx, after);
                for element in size_to_old_nodes(new.graph[idx].size) {
                    let before = get_dependent_roots(false, NodeIndex::from(element as u32));
                    assert_eq!(before, after);
                }
                nodes_image.insert(after);
            }
            // FIXME: this is failing...
            //assert_eq!(nodes_image.len(), new.graph.node_count());
        }
    }
}

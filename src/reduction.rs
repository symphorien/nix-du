// SPDX-License-Identifier: LGPL-3.0

use std::collections;
use std::hash::Hasher;
use std::{self, hash::Hash};

use petgraph::visit::{DfsPostOrder, EdgeFiltered, EdgeRef, IntoEdgeReferences};

use crate::depgraph::*;

/// Merges all the in memory roots in one root
/// noop is no in memory root is present
pub fn merge_transient_roots(mut di: DepInfos) -> DepInfos {
    use self::NodeKind::*;
    if di.graph[di.root].kind() != Dummy {
        // this graph is rooted in a fs node, no transient roots
        return di;
    }

    let targets: Vec<_> = di
        .roots()
        .filter(|&idx| di.graph[idx].kind().is_transient())
        .collect();
    if targets.is_empty() {
        return di;
    }

    let fake_root_idx = di.graph.add_node(DepNode {
        description: NodeDescription::Transient,
        size: 0,
    });
    di.graph.add_edge(di.root, fake_root_idx, ());
    for idx in targets {
        let edx = di.graph.find_edge(di.root, idx).unwrap();
        di.graph.remove_edge(edx);
        di.graph.add_edge(fake_root_idx, idx, ());
    }
    di
}

/// Transitive reduction
///
/// Handles cycles by removing back edges first, then doing tred on the resulting dag, and then
/// reapplying the back edges. This is probably not minimal, but I don't expect that many back
/// edges.
///
/// Also preserves roots.
///
/// Panics if all nodes are not reachable from the root.
pub fn transitive_reduction(mut di: DepInfos) -> DepInfos {
    let mut toposort = Vec::with_capacity(di.graph.node_count());
    let mut dfs = DfsPostOrder::new(&di.graph, di.root);
    while let Some(node) = dfs.next(&di.graph) {
        toposort.push(node)
    }
    assert_eq!(
        toposort.len(),
        di.graph.node_count(),
        "transitive_reduction called with graph with unreachable nodes"
    );
    // toposort is reverse dfs post order
    toposort.reverse();
    // invert that
    let mut reverse_topo = vec![0; di.graph.node_count()];
    for (i, node) in toposort.iter().enumerate() {
        reverse_topo[node.index()] = i;
    }
    // A DAG without the back edges.
    let is_back_edge = |e: petgraph::graph::EdgeReference<_, _>| {
        reverse_topo[e.source().index()] >= reverse_topo[e.target().index()]
    };
    let dag = petgraph::visit::EdgeFiltered::from_fn(&di.graph, |e| !is_back_edge(e));
    let (intermediate, _) =
        petgraph::algo::tred::dag_to_toposorted_adjacency_list::<_, u32>(&dag, &toposort);
    let (tred, _) = petgraph::algo::tred::dag_transitive_reduction_closure(&intermediate);

    // recreate the graph ...
    let mut new = DepGraph::with_capacity(toposort.len(), di.graph.edge_count());
    // ... with the same nodes
    for node in di.graph.node_weights_mut() {
        let mut dummy = DepNode::dummy();
        std::mem::swap(&mut dummy, node);
        new.add_node(dummy);
    }
    // ... the back edges and edges to roots
    for e in di.graph.edge_references() {
        if is_back_edge(e) || e.source() == di.root {
            new.add_edge(e.source(), e.target(), ());
        }
    }
    // ... and the edges of tred
    for e in tred.edge_references() {
        new.add_edge(
            toposort[e.source() as usize],
            toposort[e.target() as usize],
            (),
        );
    }
    std::mem::swap(&mut di.graph, &mut new);
    di
}

fn hash(state: u128, value: impl std::hash::Hash + Copy) -> u128 {
    let mut hasher = std::collections::hash_map::DefaultHasher::default();
    state.hash(&mut hasher);
    ("first", value).hash(&mut hasher);
    let hash1 = hasher.finish().to_le_bytes();
    ("second", value).hash(&mut hasher);
    let hash2 = hasher.finish().to_le_bytes();
    let result: [u8; 16] = [
        hash1[0], hash1[1], hash1[2], hash1[3], hash1[4], hash1[5], hash1[6], hash1[7], hash2[0],
        hash2[1], hash2[2], hash2[3], hash2[4], hash2[5], hash2[6], hash2[7],
    ];
    u128::from_le_bytes(result)
}

#[test]
fn test_hash() {
    // the hash is the same on all calls
    assert_eq!(hash(1, 2), hash(1, 2));
    // the hash does not look 100% broken
    assert!(hash(1, 2) != hash(1, 3));
    assert!(hash(2, 2) != hash(1, 2));
    assert!(hash(1, 1) != hash(1, 2));
}

#[test]
fn test_hash_larger() {
    const N: usize = 500;
    let mut values = std::collections::BTreeSet::new();
    for i in 0..N {
        for j in 0..N {
            values.insert(hash(i as u128, j));
        }
    }
    // test the absence of collision
    assert_eq!(values.len(), N * N);
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
/// * n+m in space
/// * (n+m)*r in time
///
/// Expected simplification: as I write theses lines, on my store (`NixOS`, 37G)
/// * before: n=37594, m=262914
/// * after `condense`: n=61, m=211
pub fn condense(mut di: DepInfos) -> DepInfos {
    // I don't like non-deterministic algorithms. they are a nightmare to debug.
    // But we rely on the hash of roots behaving like a random variable.
    // So we seed the hash with the graph.
    // Unfortunately, petgraph::Graph does not implement Hash, so let's do it
    // by hand.
    // hashing nodes is enough, if edges change then some store paths must also change.
    let mut start_hash = 0;
    for node in di.graph.raw_nodes() {
        start_hash = hash(start_hash, &node.weight);
    }

    let mut classes: Vec<u128> = vec![start_hash; di.graph.node_count()];

    // label each node with the set of roots that depend on it
    // actually we don't label each node with a set of roots indices, which would take too much
    // memory, but with the hash of this set. The probability of collision is then bounded by
    // the birthday paradox with (number of nodes) people and 2^128 days. It's very low :)
    for root in di.roots() {
        let mut bfs = petgraph::visit::Bfs::new(&di.graph, root);
        while let Some(nx) = bfs.next(&di.graph) {
            // importantly roots are visited in the same order on each node, so that the hash is
            // equal for the same set of roots
            classes[nx.index()] ^= hash(classes[nx.index()], root);
        }
    }

    let mut bfs = petgraph::visit::Bfs::new(&di.graph, di.root);

    // now remove spurious elements from the original graph.
    // removing nodes is slow, so we create a new graph for that.
    let mut new_ids = collections::BTreeMap::new(); // set of roots => new node index
    let mut new_graph = DepGraph::new();

    // we take as representative the topmost element of the class,
    // topmost as in depth -- the first reached in a BFS
    while let Some(idx) = bfs.next(&di.graph) {
        let representative = classes[idx.index()]; // hash of the set of roots that depend on this
                                                   // node
        let new_node = new_ids.entry(representative).or_insert_with(|| {
            let mut w = DepNode::dummy();
            std::mem::swap(&mut w, &mut di.graph[idx]);
            new_graph.add_node(w)
        });
        let new_w = &mut new_graph[*new_node];
        new_w.size = new_w.size + di.graph[idx].size;
    }

    let new_root = new_ids[&classes[di.root.index()]];
    // keep edges
    for edge in di.graph.raw_edges() {
        let from = new_ids[&classes[edge.source().index()]];
        if from == new_root && edge.source() != di.root {
            // this node is unreachable, so it falls into the equivalence class of the root
            continue;
        };
        let to = new_ids[&classes[edge.target().index()]];
        debug_assert_ne!(to, new_root);
        if from == to {
            // keep the graph acyclic
            continue;
        }
        new_graph.update_edge(from, to, ());
    }

    di.graph = new_graph;
    di.root = new_root;
    di.metadata.reachable = Reachability::Connected;
    di
}

/// Creates a new graph retaining only reachable nodes
pub fn keep_reachable(mut di: DepInfos) -> DepInfos {
    let mut new_graph = DepGraph::new();
    // ids of nodes put in new_graph
    let mut new_ids = collections::BTreeMap::new();

    let mut dfs = di.dfs();
    while let Some(idx) = dfs.next(&di.graph) {
        let mut new_w = DepNode::dummy();
        std::mem::swap(&mut di.graph[idx], &mut new_w);
        let new_node = new_graph.add_node(new_w);
        new_ids.insert(idx, new_node);
    }

    // keep edges
    for edge in di.graph.raw_edges() {
        if let (Some(&newfrom), Some(&newto)) =
            (new_ids.get(&edge.source()), new_ids.get(&edge.target()))
        {
            new_graph.add_edge(newfrom, newto, ());
        }
    }

    di.graph = new_graph;
    di.root = new_ids[&di.root];
    di.metadata.reachable = Reachability::Connected;
    di
}

/// Creates a new graph retaining only nodes whose weight return
/// `true` when passed to `filter`. The nodes which are dropped are
/// merged into an arbitrary parent (ie. the name is dropped, but edges and size
/// are merged). Roots which have at least a transitive child kept are kept as
/// well. Other roots (and the size gathered below) are merged in a dummy root.
///
/// Note that `filter` will be called at most once per node.
///
/// Requires that all nodes are reachable from the root.
/// `assert_eq!(di.metadata.reachable, Reachability::Connected);`
pub fn keep<T: Fn(&DepNode) -> bool>(mut di: DepInfos, filter: T) -> DepInfos {
    assert_eq!(di.metadata.reachable, Reachability::Connected);
    let mut new_graph = DepGraph::new();
    // ids of nodes put in new_graph
    let mut new_ids = collections::BTreeMap::new();
    // weights of roots which are not yet added to the graph
    // they are added on demand when we realize one of their children is kept
    let mut ondemand_weights = collections::BTreeMap::new();

    // loop over nodes to see which we keep
    for idx in di.graph.node_indices() {
        if idx == di.root || filter(&di.graph[idx]) {
            let mut new_w = DepNode::dummy();
            std::mem::swap(&mut di.graph[idx], &mut new_w);
            new_ids.insert(idx, new_graph.add_node(new_w));
        }
    }
    // store the weight of remaining roots
    let mut walker = di.roots().detach();
    while let Some(idx) = walker.next_node(&di.graph) {
        if !new_ids.contains_key(&idx) {
            let mut new_w = DepNode::dummy();
            std::mem::swap(&mut di.graph[idx], &mut new_w);
            ondemand_weights.insert(idx, new_w);
        }
    }

    // visit the old graph to add new edges accordingly
    // there is a subtlety:
    // when we visit a node, we need to know if any of its children will be kept
    // but for ondemand roots, we don"t know yet.
    // Therefore we visit nodes in reverse topological order.
    let mut toposort =
        petgraph::algo::toposort(&di.graph, None).expect("keep argument is not acyclic");
    {
        for old in toposort.drain(..).rev() {
            if old == di.root
                || !(new_ids.contains_key(&old) || ondemand_weights.contains_key(&old))
            {
                continue;
            }
            // if old is an on demand root, and we need to realise it, then
            // we cannot add it to new_ids because new_ids is borrowed.
            // We store the node id here in between.
            let mut old_id = None;
            {
                // borrow of new_ids
                // this filter visits the graph starting at old
                // stopping when reaching a kept child
                let filter_fn = |e: petgraph::graph::EdgeReference<_, _>| {
                    e.source() == old || !new_ids.contains_key(&e.source())
                };
                let mut dfs =
                    petgraph::visit::Dfs::new(&EdgeFiltered::from_fn(&di.graph, filter_fn), old);
                let old_ = dfs.next(&EdgeFiltered::from_fn(&di.graph, filter_fn)); // skip old
                debug_assert_eq!(Some(old), old_);
                while let Some(idx) = dfs.next(&EdgeFiltered::from_fn(&di.graph, filter_fn)) {
                    if let Some(&new2) = new_ids.get(&idx) {
                        // kept child
                        // let's add an edge from old to this child
                        let new = match ondemand_weights.remove(&old) {
                            Some(new_w) => {
                                // this is an ondemand root, add it to new_graph
                                let t = new_graph.add_node(new_w);
                                // we should do:
                                // new_ids.insert(old, t);
                                // but new_ids is borrowed.
                                old_id = Some(t);
                                t
                            }
                            None => old_id.unwrap_or_else(|| new_ids[&old]),
                        };
                        new_graph.add_edge(new, new2, ());
                    } else {
                        // this child is not kept
                        // absorb its size upstream
                        let wup: &mut DepNode =
                            ondemand_weights.get_mut(&old).unwrap_or_else(|| {
                                &mut new_graph[old_id.unwrap_or_else(|| new_ids[&old])]
                            });
                        wup.size = wup.size + di.graph[idx].size;
                        di.graph[idx].size = 0;
                    }
                }
            }
            if let Some(id) = old_id {
                new_ids.insert(old, id);
            };
        }
    }
    debug_assert_eq!(di.reachable_size(), 0);
    let new_root = new_ids[&di.root];
    // we add edges to kept roots
    for id in di.roots() {
        if let Some(&nid) = new_ids.get(&id) {
            new_graph.add_edge(new_root, nid, ());
        }
    }
    // to keep the size unchanged, we create a dummy root with the remaining size
    let remaining_size = ondemand_weights.values().map(|drv| drv.size).sum();
    if remaining_size > 0 {
        let fake_root = DepNode {
            description: NodeDescription::FilteredOut,
            size: remaining_size,
        };
        let id = new_graph.add_node(fake_root);
        new_graph.add_edge(new_root, id, ());
    }

    di.root = new_root;
    di.graph = new_graph;
    di.metadata.reachable = Reachability::Connected;
    di
}

#[cfg(test)]
mod tests {
    use crate::depgraph::*;
    use crate::reduction::*;
    use enum_map::enum_map;
    use petgraph::prelude::NodeIndex;
    use petgraph::visit::IntoNodeReferences;
    use petgraph::visit::NodeRef;
    use rand::distr::weighted::WeightedIndex;
    use rand::distr::Distribution;
    use rand::Rng;
    use std::collections::{self, BTreeMap, BTreeSet};

    /// asserts that `transform` preserves
    /// * the set of roots, by path
    /// * reachable size
    /// * the root, by path
    fn check_invariants<T: Fn(DepInfos) -> DepInfos>(transform: T, di: DepInfos, same_roots: bool) {
        let orig = di.clone();
        orig.check_metadata();
        let new = transform(di);
        println!(
            "OLD:\n{:?}\nNew:\n{:?}",
            petgraph::dot::Dot::new(&orig.graph),
            petgraph::dot::Dot::new(&new.graph)
        );
        new.check_metadata();
        if same_roots {
            assert_eq!(new.roots_name(), orig.roots_name(), "not the same roots");
        }
        assert_eq!(
            new.reachable_size(),
            orig.reachable_size(),
            "not the same reachable size"
        );
        assert_eq!(
            new.graph[new.root], orig.graph[orig.root],
            "not the same root"
        );
        let _ = petgraph::algo::toposort(&new.graph, None).expect("the graph has a cycle");
        assert_eq!(
            new.graph
                .neighbors_directed(new.root, petgraph::prelude::Incoming)
                .count(),
            0,
            "incoming edges to root"
        );
    }
    /// generates a random `DepInfos` where
    /// * all derivations have a distinct path
    /// * there are `size` derivations
    /// * the expected average degree of the graph should be `avg_degree`
    /// * the first 62 nodes have size `1<<index`
    ///
    /// if connected is true, forces the output to be reachable from the root
    /// otherwise, it is random.
    fn generate_random(size: u32, avg_degree: u32, connected: bool) -> DepInfos {
        use self::NodeDescription::*;
        assert!(avg_degree <= size - 1);
        let choices = &[true, false];
        let weights = &[avg_degree, size - 1 - avg_degree];
        let wc = WeightedIndex::new(weights).unwrap();
        let mut rng = rand::rng();
        let mut g: DepGraph = petgraph::graph::Graph::new();
        let rooted = rng.random();
        for i in 0..size {
            let path = i.to_string().into();
            let description = if rooted || i > 4 || rng.random() {
                Path(path)
            } else {
                if rng.random() {
                    Memory(path)
                } else {
                    Temporary(path)
                }
            };
            let size = if i < 62 {
                1u64 << i
            } else {
                3 + 2 * (i as u64)
            };
            let w = DepNode { description, size };
            g.add_node(w);
        }
        for i in 0..size {
            for j in (i + 1)..size {
                if choices[wc.sample(&mut rng)] && !g[NodeIndex::from(j)].kind().is_gc_root() {
                    g.add_edge(NodeIndex::from(i), NodeIndex::from(j), ());
                }
            }
        }
        let mut metadata = SizeMetadata {
            reachable: Reachability::Connected,
            dedup: DedupAwareness::Unaware,
            size: enum_map! { _ => enum_map!{ _ => None }},
        };
        let root = g.add_node(if rooted {
            DepNode {
                description: Path("root".into()),
                size: 42,
            }
        } else {
            DepNode::dummy()
        });
        for idx in g
            .externals(petgraph::Direction::Incoming)
            .collect::<Vec<_>>()
        {
            if !rooted && rng.random() {
                if g[idx].kind() == NodeKind::Path {
                    let w = &mut g[idx].description;
                    let mut temp = NodeDescription::Dummy;
                    std::mem::swap(&mut temp, w);
                    temp = match temp {
                        Path(path) => Link(path),
                        o => o,
                    };
                    std::mem::swap(&mut temp, w);
                    assert_eq!(w.kind(), NodeKind::Link);
                }
            }
            let make_reachable = connected || g[idx].kind().is_gc_root() || rng.random();
            if root != idx && make_reachable {
                g.add_edge(root, idx, ());
            }
            if !make_reachable {
                metadata.reachable = Reachability::Disconnected;
            }
        }
        let mut di = DepInfos {
            graph: g,
            root,
            metadata,
        };
        // there may be edges from root to root
        for i in di.roots().collect::<Vec<_>>() {
            for j in di.roots().collect::<Vec<_>>() {
                if j > i && choices[wc.sample(&mut rng)] {
                    di.graph.add_edge(i, j, ());
                }
            }
        }
        let _ = petgraph::algo::toposort(&di.graph, None).expect("the random graph has a cycle");
        di.record_metadata();
        di
    }
    fn size_to_old_nodes(drv: &DepNode) -> collections::BTreeSet<NodeIndex> {
        (0..62)
            .filter(|i| drv.size & (1u64 << i) != 0)
            .map(NodeIndex::from)
            .collect()
    }
    fn path_to_old_size(drv: &DepNode) -> u32 {
        match String::from_utf8_lossy(drv.description.path().unwrap()).parse() {
            Ok(x) => x,
            Err(_) => panic!("Cannot convert {:?}", drv.description.path().unwrap()),
        }
    }
    fn revmap(g: &DepGraph) -> BTreeMap<DepNode, NodeIndex> {
        let mut map = BTreeMap::new();
        for n in g.node_references() {
            map.insert(n.weight().clone(), n.id());
        }
        map
    }

    #[test]
    /// check that condense and keep preserve some invariants
    fn invariants() {
        for _ in 0..40 {
            let di = generate_random(250, 10, false);
            println!("testing merge_transient_roots");
            check_invariants(merge_transient_roots, di.clone(), false);
            println!("testing condense");
            check_invariants(condense, di.clone(), true);
            println!("testing keep_reachable");
            check_invariants(keep_reachable, di.clone(), true);
            println!("testing keep none");
            let trimmed = keep_reachable(di);
            check_invariants(|x| keep(x, |_| false), trimmed.clone(), false);
            println!("testing keep all");
            check_invariants(|x| keep(x, |_| true), trimmed.clone(), true);
            println!("testing tred");
            check_invariants(transitive_reduction, trimmed, true);
        }
    }
    #[test]
    fn check_merge_transient_roots() {
        use self::NodeKind::*;
        for _ in 0..40 {
            let old = generate_random(250, 10, false);
            let new = merge_transient_roots(old.clone());
            let has_transient_roots = old
                .graph
                .raw_nodes()
                .iter()
                .any(|w| w.weight.kind() == Temporary || w.weight.kind() == Memory);
            if !has_transient_roots {
                let fingerprint = |di: &DepInfos| {
                    (
                        di.root,
                        di.graph
                            .node_references()
                            .map(|n| (n.id(), n.weight().clone()))
                            .collect::<Vec<_>>(),
                        di.graph
                            .edge_references()
                            .map(|e| (e.source(), e.target()))
                            .collect::<Vec<_>>(),
                    )
                };
                assert_eq!(fingerprint(&old), fingerprint(&new));
                return;
            }
            assert_eq!(old.graph.node_count() + 1, new.graph.node_count());
            let fake_root_idx = NodeIndex::from(old.graph.node_count() as u32);
            for edge in old.graph.edge_references() {
                let old_child = &old.graph[edge.target()];
                let old_parent = &old.graph[edge.source()];
                let new_child = &new.graph[edge.target()];
                let new_parent = &new.graph[edge.source()];
                assert_eq!(old_parent, new_parent);
                assert_eq!(old_child, new_child);
                let should_disappear = edge.source() == old.root && old_child.kind().is_transient();
                assert_eq!(
                    new.graph.find_edge(edge.source(), edge.target()).is_none(),
                    should_disappear
                );
                if should_disappear {
                    assert!(new.graph.find_edge(edge.source(), fake_root_idx).is_some());
                    assert!(new.graph.find_edge(fake_root_idx, edge.target()).is_some());
                }
            }
        }
    }
    #[test]
    fn check_keep_reachable() {
        for _ in 0..40 {
            let old = generate_random(150, 1, false);
            let new = keep_reachable(old.clone());
            let old_map = revmap(&old.graph);
            let new_map = revmap(&new.graph);
            let old_w: BTreeSet<_> = old_map.keys().collect();
            let new_w: BTreeSet<_> = new_map.keys().collect();
            assert!(
                new_w.is_subset(&old_w),
                "new: {:?} \nold: {:?}",
                new_map,
                old_map
            );
            let mut space = petgraph::algo::DfsSpace::new(&old.graph);
            for (w, &i) in &old_map {
                let kept = new_map.contains_key(&w);
                let reachable =
                    petgraph::algo::has_path_connecting(&old.graph, old.root, i, Some(&mut space));
                assert_eq!(kept, reachable);
            }
            for (w, &i) in &new_map {
                for (w2, &i2) in &new_map {
                    let is_edge = new.graph.find_edge(i, i2).is_some();
                    let was_edge = old
                        .graph
                        .find_edge(*(&old_map[&w]), *(&old_map[&w2]))
                        .is_some();
                    assert_eq!(is_edge, was_edge);
                }
            }
        }
    }
    #[test]
    fn check_transitive_reduction() {
        let mut rng = rand::rng();
        for _ in 0..40 {
            let mut old = generate_random(100, 3, true);
            // make it slightly cyclic
            for _ in 1..20 {
                let from = rng.random_range(1..old.graph.node_count());
                let to = rng.random_range(1..old.graph.node_count());
                old.graph
                    .add_edge(NodeIndex::from(from as u32), NodeIndex::from(to as u32), ());
                old.check_metadata();
            }

            let new = transitive_reduction(old.clone());

            // same nodes
            assert_eq!(old.graph.node_count(), new.graph.node_count());
            for i in old.graph.node_indices() {
                assert_eq!(old.graph[i], new.graph[i]);
            }

            // edges inclusion
            for e in new.graph.edge_references() {
                assert!(
                    old.graph.find_edge(e.source(), e.target()).is_some(),
                    "edge {:?} is in result of tred but not in argument",
                    e
                );
            }

            // identical closure
            let mut old_space = petgraph::algo::DfsSpace::new(&old.graph);
            let mut new_space = petgraph::algo::DfsSpace::new(&new.graph);
            for i in old.graph.node_indices() {
                for j in old.graph.node_indices() {
                    assert_eq!(
                        petgraph::algo::has_path_connecting(&old.graph, i, j, Some(&mut old_space)),
                        petgraph::algo::has_path_connecting(&new.graph, i, j, Some(&mut new_space))
                    )
                }
            }
        }
    }

    #[test]
    fn check_condense() {
        // 62 so that each node is uniquely determined by its size, and
        // merging nodes doesn't destroy this information
        for _ in 0..80 {
            let old = generate_random(62, 10, false);
            let mut old_rev = old.graph.clone();
            old_rev.reverse();
            let new = condense(old.clone());
            let mut new_rev = new.graph.clone();
            new_rev.reverse();
            let oldroots: collections::BTreeSet<NodeIndex> = old.roots().collect();
            let newroots: collections::BTreeSet<NodeIndex> = new.roots().collect();
            let get_dependent_roots = |which, idx| {
                let grev = if which { &new_rev } else { &old_rev };
                let roots = if which { &newroots } else { &oldroots };
                let mut dfs = petgraph::visit::Dfs::new(grev, idx);
                let mut res = collections::BTreeSet::new();
                while let Some(nx) = dfs.next(grev) {
                    if roots.contains(&nx) {
                        res.extend(&size_to_old_nodes(&grev[nx]) & &oldroots);
                    }
                }
                res
            };
            let mut nodes_image = collections::BTreeSet::<collections::BTreeSet<_>>::new();
            for (idx, drv) in new.graph.node_references() {
                if idx == new.root {
                    continue;
                }
                let after = get_dependent_roots(true, idx);
                let elements = size_to_old_nodes(drv);
                for &element in &elements {
                    let before = get_dependent_roots(false, element);
                    assert_eq!(
                        before,
                        after,
                        "new:{:?} and old:{:?} do not belong to the same equivalence class ({:?} != {:?})\nOLD:\n{:?}\nNew:\n{:?}",
                        idx,
                        element,
                        after,
                        before,
                        petgraph::dot::Dot::new(&old.graph),
                        petgraph::dot::Dot::new(&new.graph)
                        );
                }
                nodes_image.insert(after);
                // here check edges
                for (idx2, drv2) in new.graph.node_references() {
                    let targets = size_to_old_nodes(drv2);
                    let should_exist = idx2 != new.root
                        && idx != idx2
                        && elements.iter().any(|&from| {
                            targets
                                .iter()
                                .any(|&to| old.graph.find_edge(from, to).is_some())
                        });
                    let exists = new.graph.find_edge(idx, idx2).is_some();
                    assert_eq!(
                        should_exist,
                        exists,
                        "edge {:?} -> {:?} is wrong (expected: {:?})\nOld:\n{:?}\nNew:\n{:?}",
                        idx,
                        idx2,
                        should_exist,
                        petgraph::dot::Dot::new(&old.graph),
                        petgraph::dot::Dot::new(&new.graph)
                    );
                }
            }
            assert_eq!(
                nodes_image.len() + 1,
                new.graph.node_count(),
                "two nodes at least have the same equivalence class\nOld\n{:?}\nNew\n{:?}",
                petgraph::dot::Dot::new(&old.graph),
                petgraph::dot::Dot::new(&new.graph)
            );
        }
    }
    #[test]
    fn check_keep() {
        let filter_drv = |drv: &DepNode| {
            let log = (drv.size as f64).log2();
            log.round() as u64 % 3 == 0 // third of the drvs
        };
        for _ in 0..50 {
            let old = generate_random(62, 1, true);
            let mut new = keep(old.clone(), &filter_drv);
            println!(
                "OLD:\n{:?}\nNew:\n{:?}",
                petgraph::dot::Dot::new(&old.graph),
                petgraph::dot::Dot::new(&new.graph)
            );
            // compute who we keep
            let old_roots: Vec<_> = old.roots().collect();
            let real_filter: collections::BTreeMap<NodeIndex, bool> = old
                .graph
                .node_references()
                .map(|(n, drv)| {
                    let mut keep = false;
                    if n == old.root {
                        (n, true)
                    } else if old_roots.contains(&n) {
                        let mut dfs = petgraph::visit::Dfs::new(&old.graph, n);
                        while let Some(idx) = dfs.next(&old.graph) {
                            if filter_drv(&old.graph[idx]) {
                                keep = true;
                                break;
                            }
                        }
                        (n, keep)
                    } else {
                        (n, filter_drv(&drv))
                    }
                })
                .collect();
            // first let's get rid of {filtered out}
            let fake_roots = new
                .graph
                .node_references()
                .filter_map(|n| {
                    if n.weight().kind() == NodeKind::FilteredOut {
                        Some(n.id())
                    } else {
                        None
                    }
                })
                .collect::<collections::BTreeSet<_>>();
            assert!(fake_roots.len() < 2, "fake_roots={:?}", fake_roots);
            if let Some(&id) = fake_roots.iter().next() {
                new.graph.remove_node(id);
            }
            // nodes:
            //   * roots
            let old_roots: collections::BTreeSet<_> = old
                .roots()
                .map(|id| old.graph[id].description.clone())
                .collect();
            let new_roots = new
                .roots()
                .map(|id| new.graph[id].description.clone())
                .collect();
            let expected_roots = old
                .roots()
                .filter_map(|id| {
                    if !real_filter[&id] {
                        None
                    } else {
                        Some(old.graph[id].description.clone())
                    }
                })
                .collect();
            assert!(old_roots.is_superset(&new_roots));
            assert!(fake_roots.len() == 1 || new_roots.is_superset(&old_roots));
            assert_eq!(new_roots, expected_roots);
            //   * labels
            let labels = |di: &DepInfos, all| {
                di.graph
                    .node_references()
                    .filter_map(|n| {
                        if all || real_filter[&n.id()] {
                            Some(n.weight().description.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<collections::BTreeSet<_>>()
            };
            assert_eq!(labels(&old, false), labels(&new, true));
            //  * size
            let filtered =
                petgraph::visit::EdgeFiltered::from_fn(&old.graph, |e| !real_filter[&e.target()]);
            let filtered2 =
                petgraph::visit::EdgeFiltered::from_fn(&old.graph, |e| !real_filter[&e.source()]);
            let mut space = petgraph::algo::DfsSpace::new(&filtered);
            for (id, drv) in new.graph.node_references() {
                if id == new.root {
                    continue;
                }
                let top = NodeIndex::from(path_to_old_size(drv));
                assert!(drv.size & (1u64 << top.index()) != 0);
                for child in size_to_old_nodes(drv) {
                    assert!(
                        petgraph::algo::has_path_connecting(
                            &filtered,
                            top,
                            child,
                            Some(&mut space)
                        ),
                        "should not have coalesced {:?} and {:?}",
                        top,
                        child
                    );
                }
                // also check edges from here
                for (id2, drv2) in new.graph.node_references() {
                    if id2 == new.root {
                        continue;
                    }
                    let bottom = NodeIndex::from(path_to_old_size(drv2));
                    let targets = size_to_old_nodes(drv2);
                    let mut path_from_here_to = |targets: collections::BTreeSet<NodeIndex>| {
                        targets.iter().any(|&target| {
                            old.graph.find_edge(top, target).is_some()
                                || old.graph.edges(top).any(|edge| {
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
                    let should_exist =
                        id != id2 && path_from_here_to([bottom].iter().cloned().collect());
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

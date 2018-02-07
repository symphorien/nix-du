// SPDX-License-Identifier: LGPL-3.0

extern crate petgraph;
extern crate memchr;

use std;
use std::collections;

use petgraph::prelude::NodeIndex;
use petgraph::visit::IntoNodeReferences;
use petgraph::visit::EdgeRef;
use petgraph::Direction::Outgoing;

use depgraph::*;

/// Returns an approximation of what `condense_exact` returns.
/// Here there are still several representative per class.
/// The tradeoff is a better complexity.
///
/// Complexity: Linear time and space.
pub fn condense(mut di: DepInfos) -> DepInfos {
    // compute articulation points, ie topmost representents of every equivalence
    // class except roots

    let mut articulations = di.roots.clone();

    let mut g = di.graph.map(|_, _| NodeIndex::end(), |_, _| ());

    for root in (&di.roots).iter().cloned() {
        let mut queue = vec![root];
        g[root] = root;
        loop {
            let v = match queue.pop() {
                None => break,
                Some(v) => v,
            };
            let mut n = g.neighbors_directed(v, Outgoing).detach();
            while let Some(w) = n.next_node(&g) {
                if w == v {
                    continue;
                }
                if g[w] == NodeIndex::end() {
                    queue.push(w);
                    g[w] = root;
                } else if g[w] != root {
                    // dependence of another root
                    articulations.push(w);
                    // stop exploration
                }
            }
        }
    }

    // compute equivalence class of every node
    for w in g.node_weights_mut() {
        *w = NodeIndex::end();
    }
    for v in &articulations {
        g[*v] = *v;
    }
    let new_size = articulations.len();
    let mut queue = articulations;
    loop {
        let v = match queue.pop() {
            None => break,
            Some(v) => v,
        };
        let current = g[v];
        let mut n = g.neighbors_directed(v, Outgoing).detach();
        while let Some(w) = n.next_node(&g) {
            if g[w] == NodeIndex::end() {
                // not yet visited
                g[w] = current;
                di.graph[current].size += di.graph[w].size;
                queue.push(w);
            }
            assert_ne!(g[w], NodeIndex::end());
        }
    }

    //println!("{:?}", Dot::with_config(&g, &[Config::EdgeNoLabel]));

    // now remove spurious elements from the original graph.
    // removing nodes is slow, so we create a new graph for that.
    let mut new_ids = collections::BTreeMap::new();
    let mut new_graph = DepGraph::with_capacity(new_size, new_size);
    for (idx, w) in g.node_references() {
        if idx == *w {
            let mut dummy = Derivation::dummy();
            std::mem::swap(&mut dummy, &mut di.graph[idx]);
            let new_node = new_graph.add_node(dummy);
            new_ids.insert(idx, new_node);
        }
    }
    for edge in g.raw_edges() {
        let from = g[edge.source()];
        let to = g[edge.target()];
        if from != NodeIndex::end() && to != NodeIndex::end() && from != to {
            new_graph.update_edge(new_ids[&from], new_ids[&to], ());
        }
    }

    di.graph = new_graph;
    di.roots = di.graph
        .node_references()
        .filter_map(|(idx, node)| if node.is_root { Some(idx) } else { None })
        .collect();

    di
}

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
pub fn condense_exact(mut di: DepInfos) -> DepInfos {
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
            new_graph.update_edge(new_ids[&from], new_ids[&to], ());
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

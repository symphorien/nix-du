extern crate petgraph;
extern crate libstore;

use std::collections;
use std::vec::Vec;
use petgraph::prelude::NodeIndex;
use petgraph::visit::IntoNodeReferences;
use std::ffi::CString;
use std;
use petgraph::Direction::Outgoing;

#[derive(Debug)]
pub struct Derivation {
    pub path: CString,
    pub size: u64,
    pub is_root: bool,
}

impl Derivation {
    /// Note: clones the string describing the path.
    pub fn new_inner(p: &libstore::Path) -> Self {
        Derivation {
            path: p.path().clone(),
            size: p.size(),
            is_root: false,
        }
    }
    pub fn new_root(p: CString) -> Self {
        let size = p.to_bytes().len() as u64; // good approximation for symlinks
        Derivation {
            path: p,
            size: size,
            is_root: true,
        }
    }
    fn dummy() -> Self {
        Derivation {
            path: CString::default(),
            size: 0,
            is_root: false,
        }
    }
}

pub type Edge = ();

pub type DepGraph = petgraph::graph::Graph<Derivation, Edge, petgraph::Directed>;

pub struct DepInfos {
    pub graph: DepGraph,
    pub roots: Vec<NodeIndex>,
}

pub fn store_to_depinfos(store: &mut libstore::Store) -> DepInfos {
    let valid_paths = store.valid_paths();
    let mut g = DepGraph::with_capacity(valid_paths.len(), valid_paths.len());
    let mut path_to_node = collections::HashMap::with_capacity(valid_paths.len());
    let mut queue = Vec::new();
    for pe in valid_paths {
        let path = pe.to_path(store);
        let node = g.add_node(Derivation::new_inner(&path));
        path_to_node.insert(path.path().clone(), node);
        queue.push((node, path));
    }
    while !queue.is_empty() {
        let (node, path) = queue.pop().unwrap();
        for dep in path.deps() {
            let child = dep.to_path(store);
            //eprintln!("{:?} child of {:?}", child.path(), path.path());
            let entry = path_to_node.entry(child.path().clone());
            let childnode = match entry {
                collections::hash_map::Entry::Vacant(e) => {
                    let new_node = g.add_node(Derivation::new_inner(&child));
                    e.insert(new_node);
                    queue.push((new_node, child));
                    new_node
                }
                collections::hash_map::Entry::Occupied(e) => *e.get(),
            };
            g.add_edge(node, childnode, ());
        }
    }

    let roots_it = store.roots();
    let mut roots = Vec::with_capacity(roots_it.len());
    for (link, path) in roots_it {
        let destnode = path_to_node[path.to_path(store).path()];
        let fromnode = g.add_node(Derivation::new_root(link));
        g.add_edge(fromnode, destnode, ());
        roots.push(fromnode);
    }

    g.shrink_to_fit();
    DepInfos { graph: g, roots }
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
/// Well, in reality, there is no unique topmost representent, so we keep
/// several ones, but you get the idea.
///
/// Complexity: Linear time and space.
///
/// Expected simplification: as I write theses lines, on my store (NixOS, 37G)
/// * before, n=50714, m=338659
/// * after, n=4689, m=32722
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

    // now remove spurious elements from the original graph.
    // removing nodes is slow, so we create a new graph for that.
    let mut new_ids = collections::BTreeMap::new();
    let mut new_graph = DepGraph::new();
    for (idx, _) in g.node_references() {
        let representative = NodeIndex::from(uf.find_mut(idx.index()));
        let new_node = new_ids.entry(representative).or_insert_with(|| {
            new_graph.add_node(Derivation::dummy())
        });
        let origw = &mut di.graph[idx];
        let neww = &mut new_graph[*new_node];
        if origw.is_root || (!neww.is_root && neww.size < origw.size) {
            std::mem::swap(origw, neww);
        }
    }
    for (idx, w) in di.graph.node_references() {
        let representative = NodeIndex::from(uf.find(idx.index()));
        new_graph[new_ids[&representative]].size += w.size;
    }
    for edge in g.raw_edges() {
        let from = NodeIndex::from(uf.find(edge.source().index()));
        let to = NodeIndex::from(uf.find(edge.target().index()));
        new_graph.update_edge(new_ids[&from], new_ids[&to], ());
    }

    di.graph = new_graph;
    di.roots = di.graph
        .node_references()
        .filter_map(|(idx, node)| if node.is_root { Some(idx) } else { None })
        .collect();

    di
}

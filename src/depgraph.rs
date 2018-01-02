extern crate petgraph;
extern crate libstore;

use std::collections;
use std::vec::Vec;
use petgraph::prelude::NodeIndex;
use std::ffi::CString;
use petgraph::Direction::Outgoing;
//use std::iter::FromIterator;
//use petgraph::dot::{Dot, Config};

type Derivation = CString;
type Edge = ();

pub type DepGraph = petgraph::graph::Graph<Derivation, Edge, petgraph::Directed>;

pub struct DepInfos {
    pub graph: DepGraph,
    pub roots: Vec<NodeIndex>
}

pub fn store_to_depinfos(store: &mut libstore::Store) -> DepInfos {
    let valid_paths = store.valid_paths();
    let mut g = DepGraph::with_capacity(valid_paths.len(), valid_paths.len());
    let mut path_to_node = collections::HashMap::with_capacity(valid_paths.len());
    let mut queue = Vec::new();
    for pe in valid_paths {
        let path = pe.to_path(store);
        let node = g.add_node(path.path().to_owned());
        path_to_node.insert(path.path().to_owned(), node);
        queue.push((node, path));
    }
    while !queue.is_empty() {
        let (node, path) = queue.pop().unwrap();
        for dep in path.deps() {
            let child = dep.to_path(store);
            //eprintln!("{:?} child of {:?}", child.path(), path.path());
            let entry = path_to_node.entry(child.path().to_owned());
            let childnode =
                match entry {
                    collections::hash_map::Entry::Vacant(e) => {
                        let new_node = g.add_node(child.path().to_owned());
                        e.insert(new_node);
                        queue.push((new_node, child));
                        new_node
                    },
                    collections::hash_map::Entry::Occupied(e) => *e.get()
                };
            g.add_edge(node, childnode, ());
        }
    }

    let roots_it = store.roots();
    let mut roots = Vec::with_capacity(roots_it.len());
    for (link, path) in roots_it {
        let destnode = path_to_node[path.to_path(store).path()];
        let fromnode = g.add_node(link);
        g.add_edge(fromnode, destnode, ());
        roots.push(fromnode);
    }

    g.shrink_to_fit();
    DepInfos{ graph: g, roots }
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
    /*
       {
       let interesting = 
       collections::BTreeSet::from_iter(
       (&di.roots).iter().filter(|v| {
       di.graph[**v].to_bytes()[1..10] != b"nix/store"[..]
       })
       );

*/

    let mut g = di.graph.map(
        |_, _| { NodeIndex::end() },
        |_, _| { () }
        );

    for root in (&di.roots).iter().cloned() {
        let mut queue = vec!(root);
        g[root] = root;
        loop {
            let v = match queue.pop() {
                None => break,
                Some(v) => v
            };
            let mut n = g.neighbors_directed(v, Outgoing).detach();
            while let Some(w) = n.next_node(&g) {
                if w == v { continue; }
                if g[w] == NodeIndex::end() {
                    /*
                       if interesting.contains(&root) {
                       eprintln!("Node {:?}({:?}) is appended to direct class of {:?}({:?})", w, di.graph[w], root, di.graph[root]);
                       }
                       */
                    queue.push(w);
                    g[w] = root;
                } else if g[w] != root {
                    // dependence of another root
                    articulations.push(w);
                    /*
                    if interesting.contains(&root) || interesting.contains(&g[w]) {
                        eprintln!("Node {:?}({:?}) is an articulation between {:?}({:?}) and {:?}({:?})", w, di.graph[w], g[w], di.graph[g[w]], root, di.graph[root]);
                    }
                    */
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
    let mut queue = articulations;
    loop {
        let v = match queue.pop() {
            None => break,
            Some(v) => v
        };
        let current = g[v];
        let mut n = g.neighbors_directed(v, Outgoing).detach();
        while let Some(w) = n.next_node(&g) {
            /*
               if interesting.contains(&current) {
               eprintln!("{:?}(color {:?}, {:?}) is a parent of {:?} (color {:?}, {:?})", v, icurrent, di.graph[current],di.graph[v], w, g[w], di.graph[w]);
               }
               */
            if g[w] == NodeIndex::end() {
                // not yet visited
                g[w] = current;
                queue.push(w);
            }
            assert_ne!(g[w], NodeIndex::end());
        }
    }

    //println!("{:?}", Dot::with_config(&g, &[Config::EdgeNoLabel]));

    // now remove spurious elements from the original graph.
    // we take into advantage that indices are shared through a map call.
    for edge in g.raw_edges() {
        let from = g[edge.source()];
        let to = g[edge.target()];
        if from != NodeIndex::end() && to != NodeIndex::end() && from != to {
            di.graph.update_edge(from, to, ());
        }
    }
    di.graph.retain_nodes( | _, idx | { g[idx] == idx } );
    /*
       }
       */

    di
}









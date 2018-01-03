extern crate petgraph;

use depgraph;
use std::io::{self, Write};
use petgraph::visit::IntoNodeReferences;

pub fn render<W: Write>(dependencies: &depgraph::DepInfos, w: &mut W) -> io::Result<()> {
    w.write_all(b"digraph nixstore {\n")?;
    w.write_all(b"node [shape = box];\n")?;
    w.write_all(b"{ rank = same;\n")?;
    for idx in &dependencies.roots {
        write!(w, "N{}; ", idx.index())?;
    }
    w.write_all(b"\n};\n")?;
    w.write_all(b"node [shape = circle];\n")?;
    for (idx, node) in dependencies.graph.node_references() {
        writeln!(w, "N{}[label={:?},narsize={}];", idx.index(), node.path, node.size)?;
    }
    for edge in dependencies.graph.raw_edges() {
        writeln!(w, "N{} -> N{};", edge.source().index(), edge.target().index())?;
    }
    w.write_all(b"}\n")?;
    Ok(())
}


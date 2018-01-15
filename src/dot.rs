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
        write!(
            w,
            "N{}[tooltip=\"",
            idx.index())?;
        w.write_all(node.path.to_bytes())?;
        write!(w,
            ", size={}\",label=\"",
            node.size)?;
        w.write_all(node.name())?;
        w.write_all(b"\"];\n")?;
    }
    for edge in dependencies.graph.raw_edges() {
        writeln!(
            w,
            "N{} -> N{};",
            edge.source().index(),
            edge.target().index()
        )?;
    }
    w.write_all(b"}\n")?;
    Ok(())
}

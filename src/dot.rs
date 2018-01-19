extern crate petgraph;
extern crate humansize;
extern crate palette;

use std;
use depgraph;
use std::io::{self, Write};
use petgraph::visit::IntoNodeReferences;
use self::palette::{Hsv, Rgb};
use self::humansize::FileSize;

pub fn render<W: Write>(dependencies: &depgraph::DepInfos, w: &mut W) -> io::Result<()> {
    // compute color gradient
    // first, min and max
    let mut min = dependencies.graph.raw_nodes()[0].weight.size;
    let mut max = min;
    for node in &dependencies.graph.raw_nodes()[1..] {
        max = std::cmp::max(node.weight.size, max);
        min = std::cmp::min(node.weight.size, min);
    }

    let scale = move |size| (((size - min) as f64) / ((max - min) as f64)) as f32;

    let gradient = palette::gradient::Gradient::new(
        vec![
            palette::named::ROYALBLUE,
            palette::named::GREENYELLOW,
            palette::named::GOLD,
            palette::named::RED,
        ].iter()
            .map(|x| {
                let color: Rgb = palette::pixel::Srgb::from_pixel(x).into();
                Hsv::from(color)
            })
            .collect::<Vec<Hsv>>(),
    );

    w.write_all(b"digraph nixstore {\n")?;
    w.write_all(b"rankdir=LR;")?;
    w.write_all(
        b"node [shape = tripleoctagon, style=filled];\n",
    )?;
    w.write_all(b"{ rank = same;\n")?;
    for idx in &dependencies.roots {
        write!(w, "N{}; ", idx.index())?;
    }
    w.write_all(b"\n};\n")?;
    w.write_all(b"node [shape = box];\n")?;
    for (idx, node) in dependencies.graph.node_references() {
        let size = node.size
            .file_size(humansize::file_size_opts::BINARY)
            .unwrap();
        let color: Hsv = gradient.get(scale(node.size));
        let textcolor = if color.value > 0.8 {
            "#000000"
        } else {
            "#ffffff"
        };
        let (r, g, b): (u8, u8, u8) = Rgb::from(color).to_pixel();
        write!(
            w,
            "N{}[color=\"#{:02X}{:02X}{:02X}\",fontcolor=\"{}\",label=\"",
            idx.index(),
            r,
            g,
            b,
            textcolor
        )?;
        w.write_all(node.name())?;
        writeln!(w, " ({})\"];", size)?;
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

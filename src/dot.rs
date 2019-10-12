// SPDX-License-Identifier: LGPL-3.0

use crate::depgraph;
use humansize::FileSize;
use palette::{FromColor, Hsv, Srgb};
use petgraph::visit::IntoNodeReferences;
use std;
use std::io::{self, Write};

pub fn render<W: Write>(dependencies: &depgraph::DepInfos, w: &mut W) -> io::Result<()> {
    // compute color gradient
    // first, min and max
    let mut min = dependencies.graph.raw_nodes()[0].weight.size.get();
    let mut max = min;
    for node in &dependencies.graph.raw_nodes()[1..] {
        max = std::cmp::max(node.weight.size.get(), max);
        min = std::cmp::min(node.weight.size.get(), min);
    }
    let span = (max - min) as f64;

    let scale = move |size| (((size - min) as f64) / span) as f32;

    let gradient = palette::gradient::Gradient::new(
        vec![
            palette::named::ROYALBLUE,
            palette::named::GREENYELLOW,
            palette::named::GOLD,
            palette::named::RED,
        ]
        .into_iter()
        .map(|x| {
            // into_format() converts from u8 to f32
            // into_linear() converts color space
            Hsv::from_rgb(x.into_format().into_linear())
        })
        .collect::<Vec<Hsv>>(),
    );

    w.write_all(b"digraph nixstore {\n")?;
    w.write_all(b"rankdir=LR;\n")?;
    w.write_all(b"node [shape = tripleoctagon, style=filled];\n")?;
    w.write_all(b"{ rank = same;\n")?;
    for idx in dependencies.roots() {
        write!(w, "N{}; ", idx.index())?;
    }
    w.write_all(b"\n};\n")?;
    w.write_all(b"node [shape = box];\n")?;
    for (idx, node) in dependencies.graph.node_references() {
        if idx == dependencies.root {
            continue;
        };
        let size = node
            .size
            .get()
            .file_size(humansize::file_size_opts::BINARY)
            .unwrap();
        let color: Hsv = gradient.get(scale(node.size.get()));
        let textcolor = if color.value > 0.8 {
            "#000000"
        } else {
            "#ffffff"
        };
        let (r, g, b): (u8, u8, u8) = Srgb::from(color).into_format().into_components();
        write!(
            w,
            "N{}[color=\"#{:02X}{:02X}{:02X}\",fontcolor=\"{}\",label=\"",
            idx.index(),
            r,
            g,
            b,
            textcolor
        )?;
        w.write_all(&node.name())?;
        writeln!(w, " ({})\"];", size)?;
    }
    for edge in dependencies.graph.raw_edges() {
        if edge.source() == dependencies.root {
            continue;
        }
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

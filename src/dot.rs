// SPDX-License-Identifier: LGPL-3.0

use crate::depgraph;
use humansize::FileSize;
use palette::{encoding::Linear, rgb::Rgb, FromColor, Hsv, IntoColor, RelativeContrast, Srgb};
use petgraph::visit::IntoNodeReferences;
use std::io::{self, Write};
use std::{self, fmt::Display};

struct GraphvizColor(Hsv<Linear<palette::encoding::Srgb>, f32>);

impl Display for GraphvizColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let color = Srgb::from_linear(Rgb::from_color(self.0));
        let (r, g, b): (u8, u8, u8) = color.into_format().into_components();
        write!(f, "#{:02X}{:02X}{:02X}", r, g, b)
    }
}

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
            // into_linear() converts to linear color space
            // into_color() converts to Hsv
            x.into_format().into_linear().into_color()
        })
        .collect::<Vec<Hsv<_, f32>>>(),
    );

    let textcolors: Vec<(Hsv<_, f32>, String)> = [palette::named::WHITE, palette::named::BLACK]
        .iter()
        .map(|&c| {
            let c = c.into_format().into_linear().into_color();
            (c, GraphvizColor(c).to_string())
        })
        .collect();

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
        let color: Hsv<_, f32> = gradient.get(scale(node.size.get()));
        let (_, textcolor) = textcolors
            .iter()
            .max_by_key(|(c, _name)| (c.get_contrast_ratio(&color) * 1000.) as u64)
            .expect("no possible textcolor");
        write!(
            w,
            "N{}[color=\"{}\",fontcolor=\"{}\",label=\"",
            idx.index(),
            GraphvizColor(color),
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

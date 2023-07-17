// SPDX-License-Identifier: LGPL-3.0

use crate::depgraph;
use bytesize::ByteSize;
use petgraph::visit::IntoNodeReferences;
use scarlet::colormap::ColorMap;
use scarlet::material_colors::MaterialPrimary;
use scarlet::{colormap::ListedColorMap, prelude::*};
use std::io::{self, Write};

pub fn render<W: Write>(dependencies: &depgraph::DepInfos, w: &mut W) -> io::Result<()> {
    // compute color gradient
    // first, min and max
    let mut min = dependencies.graph.raw_nodes()[0].weight.size;
    let mut max = min;
    for node in &dependencies.graph.raw_nodes()[1..] {
        max = std::cmp::max(node.weight.size, max);
        min = std::cmp::min(node.weight.size, min);
    }
    let span = (max - min) as f64;

    let scale = move |size| (((size - min) as f64) / span);

    let gradient = ListedColorMap::turbo();
    let textcolors: Vec<RGBColor> = [MaterialPrimary::White, MaterialPrimary::Black]
        .iter()
        .map(|&c| RGBColor::from_material_palette(c))
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
        let size = ByteSize::b(node.size);
        let offset = scale(node.size);
        // make large node more visible in the color map
        let offset = offset.sqrt();
        let color: RGBColor = gradient.transform_single(offset);
        let textcolor = textcolors
            .iter()
            .max_by_key(|c| (c.distance(&color) * 1000.) as u64)
            .expect("no possible textcolor")
            .to_string();
        write!(
            w,
            "N{}[color=\"{}\",fontcolor=\"{}\",label=\"",
            idx.index(),
            color.to_string(),
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

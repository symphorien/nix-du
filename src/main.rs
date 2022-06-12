// SPDX-License-Identifier: LGPL-3.0

use clap::Parser;
use enum_map::enum_map;

#[macro_use]
pub mod msg;
pub mod bindings;
pub mod depgraph;
pub mod dot;
pub mod opt;
pub mod reduction;
use crate::msg::*;
use bytesize::ByteSize;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

/* so that these functions are available in libnix_adepter.a */
pub use crate::depgraph::{register_edge, register_node};

#[derive(Debug, Eq, PartialEq)]
enum StatOpts {
    Full,
    Alive,
}

type OptLevel = Option<StatOpts>;

fn print_stats<W: io::Write>(w: &mut W, g: &depgraph::DepInfos) -> io::Result<()> {
    use crate::depgraph::DedupAwareness::*;
    use crate::depgraph::Reachability::*;
    let size = &g.metadata.size;
    let best = enum_map! {
        what => size[Aware][what].as_ref().or_else(|| size[Unaware][what].as_ref())
    };
    if best[Connected].is_none() && best[Disconnected].is_none() {
        return Ok(());
    }
    write!(w, "Size statistics for the ")?;
    let root = &g.graph[g.root];
    match root.description.path() {
        None => write!(w, "whole store")?,
        Some(p) => {
            write!(w, "closure of ")?;
            w.write_all(p)?
        }
    }
    writeln!(w, ":")?;
    for (what, value) in best {
        if let Some(&total) = value {
            let desc = match what {
                Disconnected => "Total",
                Connected => "Alive",
            };
            write!(w, "\t{}: {}", desc, ByteSize::b(total))?;
            if size[Aware][what].is_none() {
                writeln!(w, " (not taking optimisation into account)")?;
            } else if let Some(unopt) = size[Unaware][what] {
                writeln!(w, " ({} saved by optimisation)", ByteSize::b(unopt - total))?;
            }
        }
    }
    Ok(())
}

const LONG_ABOUT: &'static str = "
This program outputs a graph on stdout in the dot format which may help you figuring out which \
gc-roots should be removed in order to reclaim space in the nix store.

To get started, if you are interested in freeing, say, 500MB, run \
`nix-du -s 500MB | tred | dot -Tsvg > /tmp/blah.svg` and then view the result \
in a browser or dedicated software like zgrviewer.

Without options, `nix-du` outputs a graph where all nodes on which the same set of gc-roots depend \
are coalesced into one. The resulting node has the size of the sum, and the label of an arbitrary \
component. An arrow from A to B means that while A is alive, B is also alive.

As a rule of thumb, a node labeled `foo, 30KB` means that if you remove enough roots to get rid of \
this node, then you will free `30KB`. The label `foo` may or may not have a meaning.

With some options, you can filter out some more nodes to make the graph more readable. Note that \
gc-roots which don't match such filters but have a filtered-in child are kept.

The graph can be further simplified by piping it to `tred` (transitive reduction) which is usually \
provided as part of graphviz. This is strongly recommmended.

nix-du can also be used to investigate disk usage in a nix profile. With option -r PATH \
it will tell you which of the references of PATH to remove to gain space. Notably this \
can be used with the system profile on NixOS:
`nix-du -r /run/current-system/sw/ -s 500MB | tred`
or with a user wide profile:
`nix-du -r ~/.nix-profile -s 500MB | tred`
";

/// Visualise what gc-roots you should delete to free space in your nix-store
#[derive(Parser, Debug)]
#[clap(version, about, long_about = LONG_ABOUT)]
struct Args {
    /// Hide nodes below this size (a unit should be specified: -s=50MB)
    #[clap(short = 's', long, value_name = "SIZE")]
    min_size: Option<ByteSize>,

    /// Only keep the approximately N biggest nodes
    #[clap(short = 'n', long, value_name = "N", conflicts_with = "min-size")]
    nodes: Option<u32>,

    /// Consider the dependencies of PATH instead of all gc roots
    #[clap(short = 'r', long, value_name = "PATH")]
    root: Option<PathBuf>,

    /// Dump the unaltered graph read from store to the file passed as argument. Intended for debugging.
    #[clap(long, value_name = "FILE")]
    dump: Option<PathBuf>,

    /// whether to take store optimisation into account: 0: no, 1: live paths, 2: all paths (default autodetect)
    #[clap(short='O', long, value_name="N", possible_values = &["0", "1", "2", "auto"])]
    opt_level: Option<String>,

    /// Don't print informationnal messages on stderr
    #[clap(short = 'q', long)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();

    let optlevel: Option<OptLevel> = match args.opt_level.as_ref().map(String::as_str) {
        Some("0") => Some(None),
        Some("1") => Some(Some(StatOpts::Alive)),
        Some("2") => Some(Some(StatOpts::Full)),
        Some("auto") | None => None,
        _ => unreachable!(),
    };
    let root: Option<OsString> = args.root.as_ref().map(|path| {
        let path_buf = PathBuf::from(path).canonicalize().unwrap_or_else(|err| {
            die!(
                1,
                "Could not canonicalize path «{}»: {}",
                path.display(),
                err
            )
        });
        OsString::from(path_buf)
    });
    let dumpfile: Option<(std::fs::File, &PathBuf)> = args.dump.as_ref().map(|path| {
        let f = std::fs::File::create(path).unwrap_or_else(|err| {
            die!(1, "Could not open dump file «{}»: {}", path.display(), err)
        });
        (f, path)
    });

    set_quiet(args.quiet);

    /**************************************
     * end argument parsing               *
     **************************************/

    msg!("Reading dependency graph from store... ");
    let mut g = depgraph::DepInfos::read_from_store(root)
        .unwrap_or_else(|res| die!(res, "Could not read from store"));
    msg!(
        "{} nodes, {} edges read.\n",
        g.graph.node_count(),
        g.graph.edge_count()
    );

    /*************************************
     * handling of --dump
     * **********************************/

    if let Some((mut f, path)) = dumpfile {
        msg!("Dumping dependency graph to {}...", path.display());
        dot::render(&g, &mut f)
            .unwrap_or_else(|err| die!(1, "Could not dump dependency graph: {}", err));
        drop(f);
        msg!(" done\n");
    }

    /******************
     * handling or -O *
     ******************/

    let default_optlevel = Some(StatOpts::Alive);
    let optlevel = optlevel.unwrap_or_else(|| match opt::store_is_optimised(&g) {
        Err(e) => {
            eprintln!("Could not auto detect store optimisation: {}", e);
            default_optlevel
        }
        Ok(None) => default_optlevel,
        Ok(Some(true)) => Some(StatOpts::Alive),
        Ok(Some(false)) => None,
    });

    if let Some(statopts) = optlevel {
        if statopts == StatOpts::Alive {
            // drop dead paths
            g = reduction::keep_reachable(g);
        }

        msg!(
            "Looking for optimized paths... (this could take a long time, pass option -O0 to skip)\n"
        );
        opt::refine_optimized_store(&mut g)
            .unwrap_or_else(|e| eprintln!("Could not unoptimize {:?}", e));
    }

    noisy!({
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        print_stats(&mut handle, &g).expect("could not write to stderr");
    });

    /*******************
     * graph reduction *
     *******************/

    g = reduction::merge_transient_roots(g);
    msg!("Computing quotient graph... ");
    g = reduction::condense(g);

    let mut min_size = args.min_size.map(|s| s.as_u64()).unwrap_or(0);
    if let Some(n_nodes) = args.nodes {
        if (n_nodes as usize) < g.graph.node_count() {
            let mut sizes: Vec<u64> = g.graph.raw_nodes().iter().map(|n| n.weight.size).collect();
            sizes.sort_unstable();
            min_size = sizes[sizes.len().saturating_sub(n_nodes as usize)] as u64;
        }
    }

    /*******************
     * filter handling *
     *******************/

    if min_size > 0 {
        g = reduction::keep(g, |d: &depgraph::DepNode| d.size >= min_size);
    }
    msg!(
        "{} nodes, {} edges.\n",
        g.graph.node_count(),
        g.graph.edge_count()
    );

    /*******************
     * output handling *
     *******************/

    {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        match dot::render(&g, &mut handle) {
            Ok(_) => (),
            Err(ref x) if x.kind() == io::ErrorKind::BrokenPipe => (),
            Err(x) => die!(3, "While writing to stdout: {}", x),
        }
    }
}

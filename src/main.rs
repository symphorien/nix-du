// SPDX-License-Identifier: LGPL-3.0

#[macro_use]
extern crate clap;
#[macro_use]
extern crate enum_map;
extern crate human_size;
extern crate humansize;
extern crate petgraph;

#[macro_use]
pub mod msg;
pub mod depgraph;
pub mod dot;
pub mod reduction;
pub mod bindings;
pub mod opt;
use msg::*;
use std::io;
use std::path::PathBuf;
use std::ffi::OsString;
use human_size::{Size, Byte};
use humansize::FileSize;

/* so that these functions are available in libnix_adepter.a */
pub use depgraph::{register_node, register_edge};

#[derive(Debug, Eq, PartialEq)]
enum StatOpts {
    Full,
    Alive,
}

type OptLevel = Option<StatOpts>;

fn print_stats<W: io::Write>(w: &mut W, g: &depgraph::DepInfos) -> io::Result<()> {
    use depgraph::DedupAwareness::*;
    use depgraph::Reachability::*;
    let to_human_readable = |size: u64| {
        size.file_size(humansize::file_size_opts::BINARY)
            .unwrap_or("nan".to_owned())
    };
    let size = &g.metadata.size;
    let best = enum_map!{
        what => size[Aware][what].as_ref().or(size[Unaware][what].as_ref())
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
    write!(w, ":\n")?;
    for (what, value) in best {
        if let Some(&total) = value {
            let desc = match what {
                Disconnected => "Total",
                Connected => "Alive",
            };
            write!(w, "\t{}: {}", desc, to_human_readable(total))?;
            if size[Aware][what].is_none() {
                write!(w, " (not taking optimisation into account)")?;
            } else if let Some(unopt) = size[Unaware][what] {
                write!(w, " ({} saved by optimisation)", to_human_readable(unopt - total))?;
            }
            write!(w, "\n")?;
        }
    }
    Ok(())
}

fn main() {
    let matches = clap::App::new("nix-du")
        .about(
            "visualise what gc-roots you should delete to free space in your nix-store",
        )
        .long_about(
            "
This program outputs a graph on stdout in the dot format which may help you figuring out which \
gc-roots should be removed in order to reclaim space in the nix store.

To get started, if you are interested in freeing, say, 500MB, run
`nix-du -s 500MB | tred | dot -Tsvg > /tmp/blah.svg`
and then view the result in a browser or dedicated software like zgrviewer.

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
",
        )
        .version(crate_version!())
        .arg(
            clap::Arg::with_name("min-size")
                .short("s")
                .long("min-size")
                .value_name("SIZE")
                .help(
                    "Hide nodes below this size (a unit should be specified: -s=50MB)",
                )
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("nodes")
                .short("n")
                .long("nodes")
                .value_name("N")
                .conflicts_with("min-size")
                .help("Only keep the approximately N biggest nodes")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("root")
                .short("r")
                .long("root")
                .value_name("PATH")
                .help("Consider the dependencies of PATH instead of all gc roots")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("optlevel")
                .short("O")
                .long("opt-level")
                .value_name("N")
                .help("whether to take store optimisation into account: 0: no, 1: live paths, 2: all paths (default autodetect)")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .help("Don't print informationnal messages on stderr"),
        )
        .get_matches();

    let mut min_size = match matches.value_of("min-size") {
        Some(min_size_str) => {
            min_size_str
                .parse::<Size>()
                .unwrap_or_else(|_| {
                    clap::Error::value_validation_auto(
                        "The argument to --min-size is not a valid syntax. Try -s=5MB for example."
                            .to_owned(),
                    ).exit()
                })
                .into::<Byte>().value() as u64
        }
        None => 0,
    };
    let n_nodes = match matches.value_of("nodes") {
        Some(min_size_str) => {
            match min_size_str.parse::<usize>() {
                Ok(x) if x > 0 => x,
                _ => {
                    clap::Error::value_validation_auto(
                        "The argument to --nodes is not a positive integer".to_owned(),
                    ).exit()
                }
            }
        }
        None => 0,
    };
    let optlevel: Option<OptLevel> = match matches.value_of("optlevel").unwrap_or("auto") {
        "0" => Some(None),
        "1" => Some(Some(StatOpts::Alive)),
        "2" => Some(Some(StatOpts::Full)),
        "auto" => None,
        _ => clap::Error::value_validation_auto("Only -O0, -O1, -O2 exist.".to_owned()).exit(),
    };
    let root: Option<OsString> = matches.value_of("root").map(|path| {
        let path_buf = PathBuf::from(path).canonicalize().unwrap_or_else(|err| {
            die!(1, "Could not canonicalize path «{}»: {}", path, err)
        });
        OsString::from(path_buf)
    });

    set_quiet(matches.is_present("quiet"));


    /**************************************
     * end argument parsing               *
     **************************************/

    msg!("Reading dependency graph from store... ");
    let mut g = depgraph::DepInfos::read_from_store(root).unwrap_or_else(
        |res| {
            die!(res, "Could not read from store")
        },
    );
    msg!(
        "{} nodes, {} edges read.\n",
        g.graph.node_count(),
        g.graph.edge_count()
    );


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
        opt::refine_optimized_store(&mut g).unwrap_or_else(|e| {
            eprintln!("Could not unoptimize {:?}", e)
        });
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

    if n_nodes > 0 && n_nodes < g.graph.node_count() {
        let mut sizes: Vec<u64> = g.graph.raw_nodes().iter().map(|n| n.weight.size).collect();
        sizes.sort_unstable();
        min_size = sizes[sizes.len().saturating_sub(n_nodes)];
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
        dot::render(&g, &mut handle).expect("Cannot write to stdout");
    }
}

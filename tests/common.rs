extern crate cli_test_dir;
extern crate petgraph;
extern crate regex;
extern crate human_size;

use std::process::Command;
use cli_test_dir::TestDir;
use std::fs;
use petgraph::prelude::*;
use petgraph::visit::IntoNodeReferences;
use cli_test_dir::ExpectStatus;
use human_size::Size;

fn setup_nix_env(mut c: Command, t: &TestDir) -> Command {
    let store_root = t.path("nixstore");

    for &(key, value) in &[
        ("NIX_STORE_DIR", "store"),
        ("NIX_LOCALSTATE_DIR", "var"),
        ("NIX_LOG_DIR", "var/log/nix"),
        ("NIX_STATE_DIR", "var/nix"),
        ("NIX_CONF_DIR", "etc"),
        // On osx, nix uses a minimal sandbox even with --option sandbox false
        // Trouble is, setting up a sandbox inside a sandbox is forbidden and we get:
        // sandbox-exec: sandbox_apply_container: Operation not permitted
        // Let's disable this.
        ("_NIX_TEST_NO_SANDBOX", "1"),
    ]
    {
        let dir = store_root.join(value);
        fs::create_dir_all(&dir).unwrap();
        c.env(key, dir);
    }
    for key in &["NIX_REMOTE", "NIX_PATH"] {
        c.env(key, "");
    }
    c
}

pub fn call(exe: &'static str, t: &TestDir) -> Command {
    let mut c = setup_nix_env(Command::new(exe), t);
    c.current_dir(t.path("."));
    c
}

pub fn call_self(t: &TestDir) -> Command {
    let mut c = setup_nix_env(t.cmd(), t);
    c.current_dir(t.path("."));
    c
}

pub type Derivation = &'static str;
pub type Specification = petgraph::graph::Graph<Derivation, ()>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    name: String,
    count: u16,
}
pub type Output = petgraph::graph::Graph<Class, ()>;
pub fn prepare_store(spec: &Specification, t: &TestDir) {
    let mut content = format!(
        "with import {};
    rec {{
    ",
        t.src_path("tests")
            .join("nix")
            .join("template.nix")
            .to_string_lossy()
    );
    for (id, drv) in spec.node_references() {
        content += &format!(
            " {} = mkNode \"{}\" [{}];",
            drv,
            drv,
            spec.edges(id)
                .map(|e| spec[e.target()])
                .collect::<Vec<&str>>()
                .join(" ")
        );
    }
    content += "\n}";
    println!("Derivation: {}", &content);
    let pkgs = t.path("pkgs.nix");
    t.create_file(&pkgs, content);
    let roots_dir = t.path("roots");
    fs::create_dir_all(&roots_dir).unwrap();
    for root in spec.externals(petgraph::Direction::Incoming) {
        println!("Building {}", spec[root]);
        call("nix-build", t)
            .arg("--option")
            .arg("sandbox")
            .arg("false")
            .arg(&pkgs)
            .arg("-A")
            .arg(spec[root])
            .arg("-o")
            .arg(&roots_dir.join(spec[root]))
            .arg("--show-trace")
            .expect_success();
        let x = call("nix-store", t)
            .arg("-q")
            .arg("--tree")
            .arg(&roots_dir.join(spec[root]))
            .expect_success()
            .stdout;
        println!("{}", String::from_utf8_lossy(&x));
        let x = call("nix-store", t)
            .arg("--gc")
            .arg("--print-roots")
            .expect_success()
            .stdout;
        println!("{}", String::from_utf8_lossy(&x));

    }
}

fn check_syntax<T: AsRef<[u8]>>(out: T, t: &TestDir) {
    let temp = t.path("out.dot");
    t.create_file(&temp, out);
    Command::new("dot")
        .arg("-o/dev/null")
        .arg(temp)
        .expect_success();
}


pub fn parse_out(out: String) -> Output {
    let mut res = Output::new();
    let mut id_to_node = std::collections::BTreeMap::new();
    let node_re = regex::Regex::new(r#"N(\d+)\[.*label="(?:.*/)?([ {}:a-z]+) \(([^)]+)\)"#)
        .unwrap();
    let edge_re = regex::Regex::new(r"N(\d+) -> N(\d+)").unwrap();
    for node in node_re.captures_iter(&out) {
        println!("node: {:?}", node);
        assert!(node.len() == 4);
        let id: u32 = node[1].parse().unwrap();
        let name = node[2]
            .to_owned()
            .replace(" ", "_")
            .replace(":", "_")
            .replace("{", "")
            .replace("}", "");
        let size: Size = node[3].parse().unwrap(); // should be 100KB*num of deps
        let count = ((size.into_bytes() as f64) / 100_000f64) as u16;
        id_to_node.insert(id, res.add_node(Class { name, count }));
    }
    for edge in edge_re.captures_iter(&out) {
        println!("edge: {:?}", edge);
        assert!(edge.len() == 3);
        let id1: u32 = edge[1].parse().unwrap();
        let id2: u32 = edge[2].parse().unwrap();
        res.add_edge(id_to_node[&id1], id_to_node[&id2], ());
    }
    res
}

fn assert_matches_one_of(got: &Output, expected: &[&Output]) {
    assert!(
        expected.iter().any(|e|
    petgraph::algo::is_isomorphic_matching(got, e, |a, b| a == b, |_, _| true)),
    "non-isomorphic graphs.\ngot:\n{:?}\nexpected:\n{}",
    petgraph::dot::Dot::new(got),
    {
        let x: Vec<_> = expected.iter().map(|e| format!("{:?}", petgraph::dot::Dot::new(e))).collect();
        &x.join("\nOR\n")
    }
    );
}

fn assert_matches(got: &Output, expected: &Output) {
    assert_matches_one_of(got, &[expected])
}

pub fn run_and_parse(args: &'static [&'static str], t: &TestDir) -> Output {

    let stdout = call_self(&t).args(args).expect_success().stdout;
    let out = String::from_utf8_lossy(&stdout);
    println!("Got output:\n{}", &out);
    check_syntax(&stdout, &t);
    parse_out(out.into_owned())
}

/// declare a Specification variable under the identifier (first argument).
/// It contains nodes named after the first list, and edges as specified in
/// the second list.
macro_rules! dec_spec {
    ($g:ident = ($($id:ident),+ ; $($from:ident -> $to:ident),*)) => {
        let mut $g = Specification::new();
        $(
            #[allow(unused_variables)]
            let $id = $g.add_node(stringify!($id));
        )+
        $(
            $g.add_edge($from, $to, ());
        )*
    };
}

/// Same as `dec_spec!` but for `Output`.
macro_rules! dec_out {
    ($g:ident = ($($id:ident $count:expr),+ ; $($from:ident -> $to:ident),*)) => {
        let mut $g = Output::new();
        $(
            #[allow(unused_variables)]
            let $id = $g.add_node(Class { name: stringify!($id).to_owned(), count: $count });
        )+
        $(
            $g.add_edge($from, $to, ());
        )*
    };
}

macro_rules! dec_test {
    ($name:ident = | $tname:ident | $inner:block) => {
        #[test]
        fn $name() {
            let $tname = cli_test_dir::TestDir::new("nix-du", stringify!($name));

            $inner
        }
    }
}
/****************************
 * Here come the tests.
 * Beware:
 * - node names must only contain [a-z]*
 * - in the output spec, remember that the only guarantee on the label of
 * an equivalence class is "the label of the first node of the class in BFS
 * order". Design your testcases wisely.
 * - same thing with `keep`: present edges are not completely specified.
 * - all nodes without incoming edges will be roots.
 ******************************/

dec_test!(
    k2_1 = |t| {
        dec_spec!(spec = (coucou, foo, bar; coucou -> foo, bar -> foo));
        prepare_store(&spec, &t);

        dec_out!(expected = (coucou 1, bar 1, foo 1; coucou -> foo, bar -> foo));
        let real = run_and_parse(&[], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    simple = |t| {
        dec_spec!(spec = (
              coucou, foo, bar, baz, mux;
              coucou -> foo, bar -> foo, foo -> baz, coucou -> mux, mux -> baz));
        prepare_store(&spec, &t);

        dec_out!(expected = (
                coucou 2, bar 1, foo 2;
                coucou -> foo, bar -> foo));
        let real = run_and_parse(&[], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    filter_size_root_kept = |t| {
        dec_spec!(spec = (
              coucou, foo, bar, baz, mux;
              coucou -> foo, bar -> foo, foo -> baz, coucou -> mux, mux -> baz));
        prepare_store(&spec, &t);

        dec_out!(expected = (
                coucou 2, bar 1, foo 2;
                coucou -> foo, bar -> foo));
        let real = run_and_parse(&["-s=150KB"], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    filter_size_root_not_kept = |t| {
        dec_spec!(spec = (
              coucou, foo, bar, baz, mux, frob;
              coucou -> foo, bar -> foo, foo -> baz, coucou -> mux, mux -> baz));
        prepare_store(&spec, &t);

        dec_out!(expected = (
                coucou 2, bar 1, foo 2, filtered_out 1;
                coucou -> foo, bar -> foo));
        let real = run_and_parse(&["-s=150KB"], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    filter_number_root_kept = |t| {
        dec_spec!(spec = (
              coucou, foo, bar, baz, mux;
              coucou -> foo, bar -> foo, foo -> baz, coucou -> mux, mux -> baz));
        prepare_store(&spec, &t);

        dec_out!(expected = (
                coucou 2, bar 1, foo 2;
                coucou -> foo, bar -> foo));
        let real = run_and_parse(&["-n2"], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    filter_number_root_not_kept = |t| {
        dec_spec!(spec = (
              coucou, foo, bar, baz, mux, frob;
              coucou -> foo, bar -> foo, foo -> baz, coucou -> mux, mux -> baz));
        prepare_store(&spec, &t);

        dec_out!(expected = (
            coucou 2, bar 1, foo 2, filtered_out 1;
            coucou -> foo, bar -> foo));
        let real = run_and_parse(&["-n2"], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    optimise = |t| {
        dec_spec!(optimised = (
              coucou, foo, bar, blih;
              coucou -> foo)); // coucou != foo == bar == blih
        dec_spec!(not_optimised = (
              baz, qux, frob;
              baz -> qux, qux -> frob));

        prepare_store(&optimised, &t);
        call("nix-store", &t).arg("--optimise").expect_success();
        prepare_store(&not_optimised, &t);
        std::fs::remove_file(t.path("roots/blih")).expect("cannot remove roots/blih");

        let real1 = run_and_parse(&["-O1"], &t);

        dec_out!(expected_bar = (
             coucou 1, bar 0, baz 3, shared_bar 1 ;
             coucou -> shared_bar, bar -> shared_bar));
        dec_out!(expected_foo = (
             coucou 1, bar 0, baz 3, shared_foo 1 ;
             coucou -> shared_bar, bar -> shared_bar));
        dec_out!(expected_blih = (
             coucou 1, bar 0, baz 3, shared_blih 1 ;
             coucou -> shared_bar, bar -> shared_bar));
        assert_matches_one_of(&real1, &[&expected_foo, &expected_bar]);

        let real2 = run_and_parse(&["-O2"], &t);
        assert_matches_one_of(&real2, &[&expected_foo, &expected_bar, &expected_blih]);

        let real = run_and_parse(&["-O0"], &t);

        dec_out!(expected_nonopt = (coucou 2, bar 1, baz 3; ));
        assert_matches(&real, &expected_nonopt);
    }
);

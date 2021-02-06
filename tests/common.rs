// SPDX-License-Identifier: LGPL-3.0

use cli_test_dir::ExpectStatus;
use cli_test_dir::OutputExt;
use cli_test_dir::TestDir;
use human_size::{Byte, Size};
use petgraph::prelude::*;
use petgraph::visit::IntoNodeReferences;
use std::fs;
use std::os::unix::fs::symlink;
use std::process::Command;

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
    ] {
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
pub fn prepare_store(spec: &Specification, nix_conf: &'static str, t: &TestDir) {
    t.create_file("nixstore/etc/nix.conf", nix_conf);
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
    let node_re =
        regex::Regex::new(r#"N(\d+)\[.*label="(?:.*/)?([ {}:a-z]+) \(([^)]+)\)"#).unwrap();
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
        let count = ((size.into::<Byte>().value() as f64) / 100_000f64) as u16;
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
        expected
            .iter()
            .any(|e| petgraph::algo::is_isomorphic_matching(got, e, |a, b| a == b, |_, _| true)),
        "non-isomorphic graphs.\ngot:\n{:?}\nexpected:\n{}",
        petgraph::dot::Dot::new(got),
        {
            let x: Vec<_> = expected
                .iter()
                .map(|e| format!("{:?}", petgraph::dot::Dot::new(e)))
                .collect();
            &x.join("\nOR\n")
        }
    );
}

fn assert_matches(got: &Output, expected: &Output) {
    assert_matches_one_of(got, &[expected])
}

pub fn run_and_parse<'a>(args: &'a [&'a str], t: &'a TestDir) -> Output {
    let process = call_self(&t)
        .arg("--dump")
        .arg("/dev/stderr")
        .args(args)
        .expect_success();
    let out = String::from_utf8_lossy(&process.stdout);
    let err = String::from_utf8_lossy(&process.stderr);
    println!("Got output:\n{}\n{}", &err, &out);
    check_syntax(&process.stdout, &t);
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
    };
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
    keep_outputs = |t| {
        dec_spec!(spec = (foo, bar; foo -> bar));
        prepare_store(&spec, "keep-outputs = true\nkeep-derivations = false\n", &t);

        // let's make a root "drvroot" to foo.drv
        let drv_for_foo_ = call("nix-store", &t)
            .arg("--query")
            .arg("--deriver")
            .arg("roots/foo")
            .expect_success();
        let drv_for_foo = drv_for_foo_.stdout_str().trim();
        dbg!(drv_for_foo);
        fs::metadata(drv_for_foo).expect("drv_for_foo does not exist");
        symlink(drv_for_foo, t.path("roots/drvroot")).unwrap();
        symlink(
            t.path("roots/drvroot"),
            t.path("nixstore/var/nix/gcroots/root"),
        )
        .unwrap();
        // now remove foo, so foo is only kept because of drvroot -> foo.drv
        std::fs::remove_file(t.path("roots/foo")).expect("cannot remove roots/blih");
        let live_ = call("nix-store", &t)
            .arg("--gc")
            .arg("--print-live")
            .expect_success();
        let live = live_.stdout_str();
        println!("Alive paths: {}", live);
        assert!(live.contains("-foo\n"));
        // the derivation still points to foo and bar, so they are alive

        dec_out!(expected = (drvroot 2;));
        let real = run_and_parse(&[], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    k2_1 = |t| {
        dec_spec!(spec = (coucou, foo, bar; coucou -> foo, bar -> foo));
        prepare_store(&spec, "", &t);

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
        prepare_store(&spec, "", &t);

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
        prepare_store(&spec, "", &t);

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
        prepare_store(&spec, "", &t);

        dec_out!(expected = (
                coucou 2, bar 1, foo 2, filtered_out 1;
                coucou -> foo, bar -> foo));
        let real = run_and_parse(&["-s=150KB"], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    filter_number_non_root = |t| {
        dec_spec!(spec = (
                a, b, c, d, e, f;
                a -> d, b -> d, c -> e, d -> e, e -> f));
        prepare_store(&spec, "", &t);

        let real = run_and_parse(&["-n1"], &t);
        dec_out!(expected11 = (
                a 2, b 1, c 1, e 2;
                a -> e, b -> e, c -> e));
        dec_out!(expected12 = (
                a 2, b 1, c 1, e 2;
                a -> e, b -> e, c -> e, b -> a));
        dec_out!(expected21 = (
                a 1, b 2, c 1, e 2;
                a -> e, b -> e, c -> e));
        dec_out!(expected22 = (
                a 1, b 2, c 1, e 2;
                a -> e, b -> e, c -> e, a -> b));
        assert_matches_one_of(&real, &[&expected11, &expected12, &expected21, &expected22]);
    }
);

dec_test!(
    filter_size_non_root = |t| {
        dec_spec!(spec = (
                a, b, c, d, e, f;
                a -> d, b -> d, c -> e, d -> e, e -> f));
        prepare_store(&spec, "", &t);

        let real = run_and_parse(&["-s=150KB"], &t);
        dec_out!(expected11 = (
                a 2, b 1, c 1, e 2;
                a -> e, b -> e, c -> e));
        dec_out!(expected12 = (
                a 2, b 1, c 1, e 2;
                a -> e, b -> e, c -> e, b -> a));
        dec_out!(expected21 = (
                a 1, b 2, c 1, e 2;
                a -> e, b -> e, c -> e));
        dec_out!(expected22 = (
                a 1, b 2, c 1, e 2;
                a -> e, b -> e, c -> e, a -> b));
        assert_matches_one_of(&real, &[&expected11, &expected12, &expected21, &expected22]);
    }
);

dec_test!(
    filter_number_root_kept = |t| {
        dec_spec!(spec = (
              coucou, foo, bar, baz, mux;
              coucou -> foo, bar -> foo, foo -> baz, coucou -> mux, mux -> baz));
        prepare_store(&spec, "", &t);

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
        prepare_store(&spec, "", &t);

        dec_out!(expected = (
            coucou 2, bar 1, foo 2, filtered_out 1;
            coucou -> foo, bar -> foo));
        let real = run_and_parse(&["-n2"], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    autodetect_un_optimised = |t| {
        dec_spec!(spec = (
              coucou, foo, bar;
              coucou -> foo)); // coucou != foo == bar

        prepare_store(&spec, "", &t);

        let real0 = run_and_parse(&["-O0"], &t);
        let realauto = run_and_parse(&[], &t);

        dec_out!(expected = (coucou 2, bar 1;));
        assert_matches(&real0, &expected);
        assert_matches(&realauto, &expected);
    }
);

dec_test!(
    autodetect_optimised = |t| {
        dec_spec!(spec = (
              coucou, foo, bar;
              coucou -> foo)); // coucou != foo == bar

        prepare_store(&spec, "", &t);
        call("nix-store", &t).arg("--optimise").expect_success();

        let real1 = run_and_parse(&["-O1"], &t);
        let realauto = run_and_parse(&[], &t);

        dec_out!(expected_bar = (
             coucou 1, bar 0, shared_bar 1 ;
             coucou -> shared_bar, bar -> shared_bar));
        dec_out!(expected_foo = (
             coucou 1, bar 0, shared_foo 1 ;
             coucou -> shared_foo, bar -> shared_foo));
        assert_matches_one_of(&real1, &[&expected_foo, &expected_bar]);
        assert_matches_one_of(&realauto, &[&expected_foo, &expected_bar]);
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

        prepare_store(&optimised, "", &t);
        call("nix-store", &t).arg("--optimise").expect_success();
        prepare_store(&not_optimised, "", &t);
        std::fs::remove_file(t.path("roots/blih")).expect("cannot remove roots/blih");

        let real1 = run_and_parse(&["-O1"], &t);

        dec_out!(expected_bar = (
             coucou 1, bar 0, baz 3, shared_bar 1 ;
             coucou -> shared_bar, bar -> shared_bar));
        dec_out!(expected_foo = (
             coucou 1, bar 0, baz 3, shared_foo 1 ;
             coucou -> shared_foo, bar -> shared_foo));
        dec_out!(expected_blih = (
             coucou 1, bar 0, baz 3, shared_blih 1 ;
             coucou -> shared_blih, bar -> shared_blih));
        assert_matches_one_of(&real1, &[&expected_foo, &expected_bar]);

        let real2 = run_and_parse(&["-O2"], &t);
        assert_matches_one_of(&real2, &[&expected_foo, &expected_bar, &expected_blih]);

        let real = run_and_parse(&["-O0"], &t);

        dec_out!(expected_nonopt = (coucou 2, bar 1, baz 3; ));
        assert_matches(&real, &expected_nonopt);
    }
);

dec_test!(
    rooted_simple = |t| {
        dec_spec!(spec = (
            a, b, c, d, e, f, g, h, i, j;
            a->b, c->d, d->e, e->j, e->g, d->f, f->g, c->h, h->i));
        // don't keep derivations to keep things simple
        prepare_store(&spec, "keep-derivations = false\n", &t);

        dec_out!(expected = (
                e 2, g 1, f 1;
                e -> g, f -> g));

        // find the store path of d
        let out = call("nix-store", &t)
            .args(&["--gc", "--print-live"])
            .expect_success();
        let txt: &str = &String::from_utf8_lossy(&out.stdout);
        let mut path: Option<String> = None;
        for line in txt.lines() {
            if line.starts_with("/") && line.ends_with("-d") {
                path = Some(line.into());
            }
        }
        // run with -r /nix/store/hash-d
        let real = run_and_parse(&["-r", &path.unwrap()], &t);
        assert_matches(&real, &expected);
    }
);

dec_test!(
    rooted_lazy = |t| {
        dec_spec!(spec = (
              coucou, foo, bar;
              coucou -> foo)); // coucou != foo == bar

        // don't keep derivations to keep things simple
        prepare_store(&spec, "keep-derivations = false\n", &t);
        call("nix-store", &t).arg("--optimise").expect_success();

        dec_out!(expected = (foo 1;));
        // if nix-du were was reading the whole store, it would deduplicate foo and bar
        // we would get shared_something
        // this test checks we only read the closure of the root

        let path = t.path("roots/coucou");
        let root = path.to_string_lossy();
        let real = run_and_parse(&["-O2", "-r", &root], &t);
        assert_matches(&real, &expected);
    }
);

mod libstore;

fn main() {
    libstore::init_nix();
    let mut store = libstore::Store::new();
    println!("{:?}", store);
    for path in store.valid_paths() {
        println!("|- {:?}", path);
    }
}

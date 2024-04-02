fn main() {
    println!("cargo:rerun-if-changed=./src/grammar.lalrpop");

    ::lalrpop::process_root().unwrap();
}

//! Pure-Rust split timing on the corpus, no Python: cargo run --release --example bench_split
#[path = "../src/lexer_tables.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod lexer_tables;
#[path = "../src/lexer.rs"]
#[allow(dead_code)]
mod lexer;
#[path = "../src/pystr.rs"]
#[allow(dead_code)]
mod pystr;
#[path = "../src/pyunicode_tables.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod pyunicode_tables;
#[path = "../src/split.rs"]
mod split;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/corpus");
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("bench/corpus: run bench/gen_corpus.py")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .map(|e| std::fs::read_to_string(e.path()).expect("read"))
        .collect();
    files.sort();
    let _ = split::split("select 1", false);
    let mut best = f64::MAX;
    let mut n = 0;
    for _ in 0..3 {
        let t = std::time::Instant::now();
        n = files
            .iter()
            .map(|f| split::split(f, false).len())
            .sum::<usize>();
        best = best.min(t.elapsed().as_secs_f64());
    }
    println!("rust split only: {best:.3} s, {n} statements");
}

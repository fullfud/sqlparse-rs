//! Pure-Rust lexer timing on the corpus, no Python: cargo run --release --example bench_lexer
#[path = "../src/lexer.rs"]
mod lexer;
#[path = "../src/pystr.rs"]
#[allow(dead_code)]
mod pystr;
#[path = "../src/pyunicode_tables.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod pyunicode_tables;
#[path = "../src/lexer_tables.rs"]
#[allow(dead_code)]
mod lexer_tables;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/corpus");
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("bench/corpus: run bench/gen_corpus.py")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .map(|e| std::fs::read_to_string(e.path()).expect("read"))
        .collect();
    files.sort();
    let _ = lexer::tokenize("select 1"); // compile the rules outside the timing
    let mut best = f64::MAX;
    let mut tokens = 0;
    for _ in 0..3 {
        let t = std::time::Instant::now();
        tokens = files
            .iter()
            .map(|f| lexer::tokenize(f).len())
            .sum::<usize>();
        best = best.min(t.elapsed().as_secs_f64());
    }
    println!(
        "rust lexer only: {best:.3} s, {tokens} tokens, {:.2} us/token",
        best / tokens as f64 * 1e6
    );
}

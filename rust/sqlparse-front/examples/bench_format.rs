//! Pure-Rust format timing on the corpus, no Python: cargo run --release --example bench_format
#[path = "../src/filters.rs"]
mod filters;
#[path = "../src/format.rs"]
mod format;
#[path = "../src/grouping.rs"]
#[allow(dead_code)] // Stack::python is for calls from Python
mod grouping;
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
#[allow(dead_code)]
mod split;
#[path = "../src/tree.rs"]
#[allow(dead_code)]
mod tree;

fn options() -> format::Options {
    format::Options {
        keyword_case: None,
        identifier_case: None,
        strip_comments: false,
        use_space_around_operators: false,
        strip_whitespace: false,
        truncate_strings: None,
        truncate_char: "[...]".into(),
        reindent: false,
        reindent_aligned: false,
        indent_columns: false,
        indent_after_first: false,
        indent_tabs: false,
        indent_width: 2,
        wrap_after: 0,
        comma_first: false,
        compact: false,
        right_margin: None,
        output_format: None,
    }
}

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/corpus");
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("bench/corpus: run bench/gen_corpus.py")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .map(|e| std::fs::read_to_string(e.path()).expect("read"))
        .collect();
    files.sort();
    let stack = grouping::Stack::unlimited();
    let sets: Vec<(&str, format::Options)> = vec![
        ("parse", options()),
        (
            "reindent+upper",
            format::Options {
                reindent: true,
                keyword_case: Some("upper".into()),
                ..options()
            },
        ),
        (
            "strip_comments",
            format::Options {
                strip_comments: true,
                ..options()
            },
        ),
        (
            "reindent_aligned",
            format::Options {
                reindent_aligned: true,
                ..options()
            },
        ),
    ];
    for (name, o) in &sets {
        let mut best = f64::MAX;
        let mut n = 0;
        for _ in 0..3 {
            let t = std::time::Instant::now();
            n = if *name == "parse" {
                files
                    .iter()
                    .map(|f| format::parse(f, &stack).expect("parse").len())
                    .sum::<usize>()
            } else {
                files
                    .iter()
                    .map(|f| format::format(f, o, &stack).expect("format").len())
                    .sum::<usize>()
            };
            best = best.min(t.elapsed().as_secs_f64());
        }
        println!("rust {name}: {best:.3} s ({n})");
    }
}

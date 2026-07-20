use perf_bench::Bencher;

fn fib(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fib(n - 1) + fib(n - 2),
    }
}

fn main() {
    let mut bencher = Bencher::default();
    let n = 26;
    let report = bencher.run(&format!("fib({n})"), n, |n| fib(*n));
    println!("{report}");
    report.write_plots("plots").expect("failed to write plots");
    std::fs::write("index.html", report.to_html()).expect("failed to write index.html");
}

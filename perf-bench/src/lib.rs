use std::fmt;
use std::fs;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use crate::collect::{PerfStats, collect_stats};

mod collect;

pub struct Bencher {
    warmup_iterations: usize,
    iterations: usize,
}

impl Default for Bencher {
    fn default() -> Self {
        Self {
            warmup_iterations: 2_000,
            iterations: 2_000,
        }
    }
}

impl Bencher {
    pub fn run<I, O, F: FnMut(&I) -> O>(&mut self, name: &str, input: I, mut f: F) -> Report {
        let input = black_box(input);
        for _ in 0..self.warmup_iterations {
            let _ = black_box(f(&input));
        }
        let stats = collect_stats(self.iterations, input, |i| {
            let _ = black_box(f(i));
        })
        .unwrap();
        let summary = PerfStatsSummary::from_stats(&stats).unwrap();
        Report {
            name: name.to_string(),
            summary,
            datapoints: stats,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatSummary {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
}

impl fmt::Display for StatSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2}",
            self.min, self.max, self.mean, self.median, self.std_dev
        )
    }
}

impl StatSummary {
    pub fn from_u64(samples: impl IntoIterator<Item = u64>) -> Option<Self> {
        let samples: Vec<u64> = samples.into_iter().collect();
        if samples.is_empty() {
            return None;
        }
        let count = samples.len();
        let min = *samples.iter().min()? as f64;
        let max = *samples.iter().max()? as f64;
        let sum: f64 = samples.iter().map(|&x| x as f64).sum();
        let mean = sum / count as f64;

        let mut sorted = samples.clone();
        sorted.sort_unstable();
        let median = if count % 2 == 1 {
            sorted[count / 2] as f64
        } else {
            (sorted[count / 2 - 1] as f64 + sorted[count / 2] as f64) / 2.0
        };

        let variance = if count <= 1 {
            0.0
        } else {
            samples
                .iter()
                .map(|&x| {
                    let diff = x as f64 - mean;
                    diff * diff
                })
                .sum::<f64>()
                / (count - 1) as f64
        };
        let std_dev = variance.sqrt();

        Some(Self {
            count,
            min,
            max,
            mean,
            median,
            std_dev,
        })
    }

    pub fn from_f64(samples: impl IntoIterator<Item = f64>) -> Option<Self> {
        let samples: Vec<f64> = samples.into_iter().collect();
        if samples.is_empty() {
            return None;
        }
        let count = samples.len();
        let min = *samples.iter().min_by(|a, b| a.partial_cmp(b).unwrap())?;
        let max = *samples.iter().max_by(|a, b| a.partial_cmp(b).unwrap())?;
        let sum: f64 = samples.iter().sum();
        let mean = sum / count as f64;

        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if count % 2 == 1 {
            sorted[count / 2]
        } else {
            (sorted[count / 2 - 1] + sorted[count / 2]) / 2.0
        };

        let variance = if count <= 1 {
            0.0
        } else {
            samples
                .iter()
                .map(|&x| {
                    let diff = x - mean;
                    diff * diff
                })
                .sum::<f64>()
                / (count - 1) as f64
        };
        let std_dev = variance.sqrt();

        Some(Self {
            count,
            min,
            max,
            mean,
            median,
            std_dev,
        })
    }
}

#[derive(Clone, PartialEq)]
pub struct PerfStatsSummary {
    pub cpu_cycles: StatSummary,
    pub instructions: StatSummary,
    pub cache_references: StatSummary,
    pub cache_misses: StatSummary,
    pub branch_instructions: StatSummary,
    pub branch_misses: StatSummary,
    pub ipc: StatSummary,
    pub branch_miss_rate: StatSummary,
    pub branch_instruction_rate: StatSummary,
    pub cache_miss_rate: StatSummary,
}

impl PerfStatsSummary {
    pub fn from_stats(stats: &[PerfStats]) -> Option<Self> {
        if stats.is_empty() {
            return None;
        }
        Some(Self {
            cpu_cycles: StatSummary::from_u64(stats.iter().map(|s| s.cpu_cycles))?,
            instructions: StatSummary::from_u64(stats.iter().map(|s| s.instructions))?,
            cache_references: StatSummary::from_u64(stats.iter().map(|s| s.cache_references))?,
            cache_misses: StatSummary::from_u64(stats.iter().map(|s| s.cache_misses))?,
            branch_instructions: StatSummary::from_u64(
                stats.iter().map(|s| s.branch_instructions),
            )?,
            branch_misses: StatSummary::from_u64(stats.iter().map(|s| s.branch_misses))?,
            ipc: StatSummary::from_f64(stats.iter().map(|s| s.ipc()))?,
            branch_miss_rate: StatSummary::from_f64(stats.iter().map(|s| s.branch_miss_rate()))?,
            branch_instruction_rate: StatSummary::from_f64(stats.iter().map(|s| s.branch_instruction_rate()))?,
            cache_miss_rate: StatSummary::from_f64(stats.iter().map(|s| s.cache_miss_rate()))?,
        })
    }
}

impl fmt::Debug for PerfStatsSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PerfStatsSummary")
            .field("cpu_cycles", &self.cpu_cycles)
            .field("instructions", &self.instructions)
            .field("cache_references", &self.cache_references)
            .field("cache_misses", &self.cache_misses)
            .field("branch_instructions", &self.branch_instructions)
            .field("branch_misses", &self.branch_misses)
            .field("ipc", &self.ipc)
            .field("branch_miss_rate", &self.branch_miss_rate)
            .field("branch_instruction_rate", &self.branch_instruction_rate)
            .field("cache_miss_rate", &self.cache_miss_rate)
            .finish()
    }
}

impl fmt::Display for PerfStatsSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "samples: {}", self.cpu_cycles.count)?;
        writeln!(f)?;
        writeln!(f, "Key Metrics (median):")?;
        writeln!(
            f,
            "  {:<30} {:>16}",
            "instructions",
            format_with_commas(self.instructions.median)
        )?;
        writeln!(
            f,
            "  {:<30} {:>16.2}",
            "instructions per cycle",
            self.ipc.median
        )?;
        writeln!(
            f,
            "  {:<30} {:>15.1}%",
            "branch instruction rate",
            self.branch_instruction_rate.median * 100.0
        )?;
        writeln!(
            f,
            "  {:<30} {:>15.1}%",
            "branch miss rate",
            self.branch_miss_rate.median * 100.0
        )?;
        writeln!(
            f,
            "  {:<30} {:>16}",
            "cache references",
            format_with_commas(self.cache_references.median)
        )?;
        writeln!(
            f,
            "  {:<30} {:>15.1}%",
            "cache miss rate",
            self.cache_miss_rate.median * 100.0
        )?;
        writeln!(f)?;
        writeln!(f, "Full Statistics:")?;
        writeln!(
            f,
            "  {:<20} {:>14} {:>14} {:>14} {:>14} {:>14}",
            "metric", "min", "max", "mean", "median", "std_dev"
        )?;
        writeln!(
            f,
            "  {:<20} {:>14.2} {:>14.2} {:>14.2} {:>14.2} {:>14.2}",
            "cpu cycles",
            self.cpu_cycles.min,
            self.cpu_cycles.max,
            self.cpu_cycles.mean,
            self.cpu_cycles.median,
            self.cpu_cycles.std_dev,
        )?;
        writeln!(
            f,
            "  {:<20} {:>14.2} {:>14.2} {:>14.2} {:>14.2} {:>14.2}",
            "instructions",
            self.instructions.min,
            self.instructions.max,
            self.instructions.mean,
            self.instructions.median,
            self.instructions.std_dev,
        )?;
        writeln!(
            f,
            "  {:<20} {:>14.2} {:>14.2} {:>14.2} {:>14.2} {:>14.2}",
            "cache references",
            self.cache_references.min,
            self.cache_references.max,
            self.cache_references.mean,
            self.cache_references.median,
            self.cache_references.std_dev,
        )?;
        writeln!(
            f,
            "  {:<20} {:>14.2} {:>14.2} {:>14.2} {:>14.2} {:>14.2}",
            "cache misses",
            self.cache_misses.min,
            self.cache_misses.max,
            self.cache_misses.mean,
            self.cache_misses.median,
            self.cache_misses.std_dev,
        )?;
        writeln!(
            f,
            "  {:<20} {:>14.2} {:>14.2} {:>14.2} {:>14.2} {:>14.2}",
            "branch instr",
            self.branch_instructions.min,
            self.branch_instructions.max,
            self.branch_instructions.mean,
            self.branch_instructions.median,
            self.branch_instructions.std_dev,
        )?;
        writeln!(
            f,
            "  {:<20} {:>14.2} {:>14.2} {:>14.2} {:>14.2} {:>14.2}",
            "branch misses",
            self.branch_misses.min,
            self.branch_misses.max,
            self.branch_misses.mean,
            self.branch_misses.median,
            self.branch_misses.std_dev,
        )?;
        writeln!(
            f,
            "  {:<20} {:>14.2} {:>14.2} {:>14.2} {:>14.2} {:>14.2}",
            "ipc",
            self.ipc.min,
            self.ipc.max,
            self.ipc.mean,
            self.ipc.median,
            self.ipc.std_dev,
        )?;
        writeln!(
            f,
            "  {:<20} {:>13.2}% {:>13.2}% {:>13.2}% {:>13.2}% {:>13.2}%",
            "branch miss rate",
            self.branch_miss_rate.min * 100.0,
            self.branch_miss_rate.max * 100.0,
            self.branch_miss_rate.mean * 100.0,
            self.branch_miss_rate.median * 100.0,
            self.branch_miss_rate.std_dev * 100.0,
        )?;
        writeln!(
            f,
            "  {:<20} {:>13.2}% {:>13.2}% {:>13.2}% {:>13.2}% {:>13.2}%",
            "branch instr rate",
            self.branch_instruction_rate.min * 100.0,
            self.branch_instruction_rate.max * 100.0,
            self.branch_instruction_rate.mean * 100.0,
            self.branch_instruction_rate.median * 100.0,
            self.branch_instruction_rate.std_dev * 100.0,
        )?;
        write!(
            f,
            "  {:<20} {:>13.2}% {:>13.2}% {:>13.2}% {:>13.2}% {:>13.2}%",
            "cache miss rate",
            self.cache_miss_rate.min * 100.0,
            self.cache_miss_rate.max * 100.0,
            self.cache_miss_rate.mean * 100.0,
            self.cache_miss_rate.median * 100.0,
            self.cache_miss_rate.std_dev * 100.0,
        )
    }
}

pub struct Report {
    name: String,
    summary: PerfStatsSummary,
    datapoints: Vec<PerfStats>,
}

impl Report {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn summary(&self) -> &PerfStatsSummary {
        &self.summary
    }

    pub fn to_html(&self) -> String {
        let mut html = String::new();
        html.push_str("<!DOCTYPE html>\n");
        html.push_str("<html lang=\"en\">\n");
        html.push_str("<head>\n");
        html.push_str("<meta charset=\"UTF-8\">\n");
        html.push_str(&format!("<title>{}</title>\n", self.name));
        html.push_str("<style>\n");
        html.push_str("  body { font-family: sans-serif; max-width: 900px; margin: 0 auto; padding: 20px; }\n");
        html.push_str(
            "  pre { background: #f5f5f5; padding: 15px; border-radius: 4px; overflow-x: auto; }\n",
        );
        html.push_str("  .chart { margin-bottom: 20px; }\n");
        html.push_str("  .chart svg { max-width: 100%; height: auto; }\n");
        html.push_str(
            "  h2 { color: #333; border-bottom: 1px solid #ddd; padding-bottom: 5px; }\n",
        );
        html.push_str("</style>\n");
        html.push_str("</head>\n");
        html.push_str("<body>\n");
        html.push_str(&format!("<h1>{}</h1>\n", self.name));
        html.push_str("<h2>Summary Statistics</h2>\n");
        html.push_str("<pre>\n");
        html.push_str(&format!("{}", self.summary));
        html.push_str("</pre>\n");
        html.push_str("<h2>Plots</h2>\n");

        if self.datapoints.is_empty() {
            html.push_str("</body>\n</html>\n");
            return html;
        }

        let cpu_cycles: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.cpu_cycles as f64)
            .collect();
        let instructions: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.instructions as f64)
            .collect();
        let cache_references: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.cache_references as f64)
            .collect();
        let cache_misses: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.cache_misses as f64)
            .collect();
        let branch_instructions: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.branch_instructions as f64)
            .collect();
        let branch_misses: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.branch_misses as f64)
            .collect();
        let ipc: Vec<f64> = self.datapoints.iter().map(|s| s.ipc()).collect();
        let branch_miss_rate: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.branch_miss_rate())
            .collect();
        let cache_miss_rate: Vec<f64> = self.datapoints.iter().map(|s| s.cache_miss_rate()).collect();
        let branch_instruction_rate: Vec<f64> = self
            .datapoints
            .iter()
            .map(|s| s.branch_instruction_rate())
            .collect();

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "cpu_cycles",
            &cpu_cycles,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "instructions",
            &instructions,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "cache_references",
            &cache_references,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "cache_misses",
            &cache_misses,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "branch_instructions",
            &branch_instructions,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "branch_misses",
            &branch_misses,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(&self.name, "ipc", &ipc));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "branch_miss_rate",
            &branch_miss_rate,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "branch_instruction_rate",
            &branch_instruction_rate,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_cumulative_line_chart(
            &self.name,
            "cache_miss_rate",
            &cache_miss_rate,
        ));
        html.push_str("</div>\n");

        html.push_str("<h3>Distributions</h3>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "cpu_cycles",
            &cpu_cycles,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "instructions",
            &instructions,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "cache_references",
            &cache_references,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "cache_misses",
            &cache_misses,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "branch_instructions",
            &branch_instructions,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "branch_misses",
            &branch_misses,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "branch_instruction_rate",
            &branch_instruction_rate,
        ));
        html.push_str("</div>\n");

        html.push_str("<div class=\"chart\">\n");
        html.push_str(&gnuplot_distribution_chart(
            &self.name,
            "cache_miss_rate",
            &cache_miss_rate,
        ));
        html.push_str("</div>\n");

        html.push_str("</body>\n");
        html.push_str("</html>\n");

        html
    }

    pub fn write_plots(&self, dir: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(dir.as_ref())?;
        let dir = fs::canonicalize(dir.as_ref())?;
        if self.datapoints.is_empty() {
            return Ok(());
        }
        let safe_name = sanitize_filename(&self.name);

        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "cpu cycles",
            |s| s.cpu_cycles as f64,
        )?;
        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "instructions",
            |s| s.instructions as f64,
        )?;
        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "cache references",
            |s| s.cache_references as f64,
        )?;
        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "cache misses",
            |s| s.cache_misses as f64,
        )?;
        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "branch instructions",
            |s| s.branch_instructions as f64,
        )?;
        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "branch misses",
            |s| s.branch_misses as f64,
        )?;
        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "branch instruction rate",
            |s| s.branch_instruction_rate(),
        )?;
        write_metric_plot(
            &self.datapoints,
            &self.name,
            &dir,
            &safe_name,
            "cache miss rate",
            |s| s.cache_miss_rate(),
        )?;

        Ok(())
    }
}

fn write_metric_plot<F: Fn(&PerfStats) -> f64>(
    datapoints: &[PerfStats],
    bench_name: &str,
    dir: &Path,
    safe_name: &str,
    metric_label: &str,
    extract: F,
) -> io::Result<()> {
    let metric_safe = metric_label.replace(' ', "_");
    let base = format!("{}_{}", safe_name, metric_safe);
    let hist_path = dir.join(format!("{}_hist.dat", base));
    let sorted_path = dir.join(format!("{}_sorted.dat", base));
    let gp_path = dir.join(format!("{}.gp", base));

    let mut values: Vec<f64> = datapoints.iter().map(extract).collect();
    if values.is_empty() {
        return Ok(());
    }

    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let (n_bins, bin_width) = if (max - min) < 1e-10 {
        (1, 1.0)
    } else {
        let bins = 30;
        (bins, (max - min) / bins as f64)
    };

    let mut bins = vec![0usize; n_bins];
    for &v in &values {
        let mut idx = ((v - min) / bin_width).floor() as usize;
        if idx >= n_bins {
            idx = n_bins - 1;
        }
        bins[idx] += 1;
    }

    {
        let mut f = fs::File::create(&hist_path)?;
        for (i, &count) in bins.iter().enumerate() {
            let center = min + (i as f64 + 0.5) * bin_width;
            writeln!(f, "{:.6} {}", center, count)?;
        }
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    {
        let mut f = fs::File::create(&sorted_path)?;
        for v in &values {
            writeln!(f, "{:.6}", v)?;
        }
    }

    let svg_path = dir.join(format!("{}.svg", base));
    {
        let mut f = fs::File::create(&gp_path)?;
        writeln!(f, "set terminal svg size 800,600 enhanced")?;
        writeln!(f, "set output '{}'", svg_path.display())?;
        writeln!(f)?;
        writeln!(f, "set multiplot layout 2,1")?;
        writeln!(f)?;
        writeln!(
            f,
            "set title '{} distribution - {}'",
            metric_label, bench_name
        )?;
        writeln!(f, "set xlabel '{}'", metric_label)?;
        writeln!(f, "set ylabel 'Count'")?;
        writeln!(f, "set style fill solid 0.4 noborder")?;
        writeln!(f, "set boxwidth {}", bin_width * 0.9)?;
        writeln!(
            f,
            "plot '{}' using 1:2 with boxes lc rgb '#4a90d9' notitle",
            hist_path.display()
        )?;
        writeln!(f)?;
        writeln!(f, "set title '{} sorted - {}'", metric_label, bench_name)?;
        writeln!(f, "set xlabel 'Sample (sorted)'")?;
        writeln!(f, "set ylabel '{}'", metric_label)?;
        writeln!(f, "unset style")?;
        writeln!(
            f,
            "plot '{}' using 0:1 with points pt 7 ps 0.3 lc rgb '#e05050' notitle",
            sorted_path.display()
        )?;
        writeln!(f)?;
        writeln!(f, "unset multiplot")?;
    }

    let output = Command::new("gnuplot").arg(&gp_path).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::new(io::ErrorKind::Other, stderr.into_owned()));
    }

    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| match c {
            '(' | ')' | ' ' | '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '.' => '_',
            c => c,
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    s.trim_matches('_').to_string()
}

fn format_with_commas(n: f64) -> String {
    let rounded = n.round() as i64;
    let s = rounded.abs().to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if rounded < 0 {
        format!("-{}", result)
    } else {
        result
    }
}

fn gnuplot_cumulative_line_chart(name: &str, ylabel: &str, points: &[f64]) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let name_clean = name.replace('_', "-");
    let ylabel_clean = ylabel.replace('_', "-");

    let mut script = String::new();
    script.push_str("set terminal svg size 800,250\n");
    script.push_str("set output '/dev/stdout'\n");
    script.push_str(&format!("set title '{} - {}'\n", name_clean, ylabel_clean));
    script.push_str("set xlabel 'Sample'\n");
    script.push_str(&format!("set ylabel '{}'\n", ylabel_clean));
    script.push_str("unset key\n");
    script.push_str("plot '-' with lines lw 1\n");
    let mut cum_sum = 0.0;
    for (i, &val) in points.iter().enumerate() {
        cum_sum += val;
        script.push_str(&format!("{} {}\n", i + 1, cum_sum));
    }
    script.push_str("e\n");

    let mut child = Command::new("gnuplot")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gnuplot");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(script.as_bytes())
        .expect("failed to write to gnuplot stdin");

    let output = child.wait_with_output().expect("failed to wait on gnuplot");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return format!("<!-- gnuplot error: {} -->", stderr.escape_default());
    }

    let svg = String::from_utf8_lossy(&output.stdout);
    if let Some(svg_start) = svg.find("<svg") {
        svg[svg_start..].to_string()
    } else {
        String::new()
    }
}

fn gnuplot_distribution_chart(name: &str, ylabel: &str, points: &[f64]) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let name_clean = name.replace('_', "-");
    let ylabel_clean = ylabel.replace('_', "-");

    if points.is_empty() {
        return String::new();
    }

    let min = points.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = points.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let (n_bins, bin_width) = if (max - min) < 1e-10 {
        (1, 1.0)
    } else {
        (30, (max - min) / 30.0)
    };

    let mut bins = vec![0usize; n_bins];
    for &v in points {
        let mut idx = ((v - min) / bin_width).floor() as usize;
        if idx >= n_bins {
            idx = n_bins - 1;
        }
        bins[idx] += 1;
    }

    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut script = String::new();
    script.push_str("set terminal svg size 800,500 enhanced\n");
    script.push_str("set output '/dev/stdout'\n");
    script.push_str(&format!(
        "set title '{} distribution - {}'\n",
        ylabel_clean, name_clean
    ));
    script.push_str("set multiplot layout 2,1\n");
    script.push_str(&format!("set xlabel '{}'\n", ylabel_clean));
    script.push_str("set ylabel 'Count'\n");
    script.push_str("set style fill solid 0.4 noborder\n");
    script.push_str(&format!("set boxwidth {}\n", bin_width * 0.9));
    script.push_str("plot '-' with boxes lc rgb '#4a90d9' notitle\n");
    for (i, &count) in bins.iter().enumerate() {
        let center = min + (i as f64 + 0.5) * bin_width;
        script.push_str(&format!("{:.6} {}\n", center, count));
    }
    script.push_str("e\n");
    script.push_str("set title ''\n");
    script.push_str("set xlabel 'Sample (sorted)'\n");
    script.push_str(&format!("set ylabel '{}'\n", ylabel_clean));
    script.push_str("unset style\n");
    script.push_str("plot '-' with points pt 7 ps 0.3 lc rgb '#e05050' notitle\n");
    for (i, &v) in sorted.iter().enumerate() {
        script.push_str(&format!("{} {}\n", i, v));
    }
    script.push_str("e\n");
    script.push_str("unset multiplot\n");

    let mut child = Command::new("gnuplot")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gnuplot");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(script.as_bytes())
        .expect("failed to write to gnuplot stdin");

    let output = child.wait_with_output().expect("failed to wait on gnuplot");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return format!("<!-- gnuplot error: {} -->", stderr.escape_default());
    }

    let svg = String::from_utf8_lossy(&output.stdout);
    if let Some(svg_start) = svg.find("<svg") {
        svg[svg_start..].to_string()
    } else {
        String::new()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.name)?;
        write!(f, "{}", self.summary)
    }
}

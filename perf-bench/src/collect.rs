use perf_event::events::Hardware;
use perf_event::{Builder, Group};
use std::fmt;

#[inline(always)]
pub fn collect_stats<I, F: FnMut(&I)>(
    iterations: usize,
    input: I,
    mut f: F,
) -> std::io::Result<Vec<PerfStats>> {
    let mut group = Group::new()?;
    let cpu_cycles = Builder::new()
        .group(&mut group)
        .kind(Hardware::CPU_CYCLES)
        .build()?;
    let instructions = Builder::new()
        .group(&mut group)
        .kind(Hardware::INSTRUCTIONS)
        .build()?;
    let cache_references = Builder::new()
        .group(&mut group)
        .kind(Hardware::CACHE_REFERENCES)
        .build()?;
    let cache_misses = Builder::new()
        .group(&mut group)
        .kind(Hardware::CACHE_MISSES)
        .build()?;
    let branch_instructions = Builder::new()
        .group(&mut group)
        .kind(Hardware::BRANCH_INSTRUCTIONS)
        .build()?;
    let branch_misses = Builder::new()
        .group(&mut group)
        .kind(Hardware::BRANCH_MISSES)
        .build()?;

    let mut results = Vec::with_capacity(iterations);
    for _ in 0..(iterations / 100).min(1) {
        f(&input);
    }
    for _ in 0..iterations {
        group.reset()?;
        group.enable()?;
        f(&input);
        group.disable()?;

        let counts = group.read()?;
        results.push(PerfStats {
            cpu_cycles: counts[&cpu_cycles],
            instructions: counts[&instructions],
            cache_references: counts[&cache_references],
            cache_misses: counts[&cache_misses],
            branch_instructions: counts[&branch_instructions],
            branch_misses: counts[&branch_misses],
        });
    }
    Ok(results)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PerfStats {
    pub cpu_cycles: u64,
    pub instructions: u64,
    pub cache_references: u64,
    pub cache_misses: u64,
    pub branch_instructions: u64,
    pub branch_misses: u64,
}

impl PerfStats {
    pub fn ipc(&self) -> f64 {
        if self.cpu_cycles == 0 {
            0.0
        } else {
            self.instructions as f64 / self.cpu_cycles as f64
        }
    }

    pub fn branch_miss_rate(&self) -> f64 {
        if self.branch_instructions == 0 {
            0.0
        } else {
            self.branch_misses as f64 / self.branch_instructions as f64
        }
    }

    pub fn branch_instruction_rate(&self) -> f64 {
        if self.instructions == 0 {
            0.0
        } else {
            self.branch_instructions as f64 / self.instructions as f64
        }
    }

    pub fn cache_miss_rate(&self) -> f64 {
        if self.cache_references == 0 {
            0.0
        } else {
            self.cache_misses as f64 / self.cache_references as f64
        }
    }
}

impl fmt::Debug for PerfStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PerfStats")
            .field("cpu_cycles", &self.cpu_cycles)
            .field("instructions", &self.instructions)
            .field("cache_references", &self.cache_references)
            .field("cache_misses", &self.cache_misses)
            .field("branch_instructions", &self.branch_instructions)
            .field("branch_misses", &self.branch_misses)
            .field("ipc", &format_args!("{:.2}", self.ipc()))
            .field(
                "branch_miss_rate",
                &format_args!("{:.2}%", self.branch_miss_rate() * 100.0),
            )
            .finish()
    }
}

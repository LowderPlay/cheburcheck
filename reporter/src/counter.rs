use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;
use log::info;
use reports::Evidence;
use crate::Verbosity;

#[derive(Default)]
pub struct Counter {
    ok: usize,
    block: usize,
    err: usize,
    pub early: usize,
    pub results: HashMap<String, Evidence>,
}

impl Counter {
    pub fn save_results(&self, output: &PathBuf) -> anyhow::Result<()> {
        let mut out = csv::WriterBuilder::new().from_path(output)?;
        out.write_record(&["target", "evidence"])?;
        for (target, evidence) in &self.results {
            out.write_record(&[target, &evidence.to_string()])?;
        }
        info!("Saved results to {:?}", output);
        Ok(())
    }

    pub fn print_results(&self, verbosity: &Verbosity) {
        if verbosity > &Verbosity::Silent {
            info!("Results:");
            for (target, evidence) in &self.results {
                match evidence {
                    Evidence::Ok if verbosity >= &Verbosity::All => println!("    [Ok] {}", target),
                    Evidence::Blocked if verbosity >= &Verbosity::Block => println!("    [Blocked] {}", target),
                    Evidence::ConnectError if verbosity >= &Verbosity::Error => println!("    [ConnectError] {}", target),
                    _ => {}
                }
            }
        }
    }
    pub fn total(&self) -> usize {
        self.ok + self.block + self.err
    }

    pub fn add(&mut self, target: &str, evidence: Evidence) {
        match evidence {
            Evidence::Ok => self.ok += 1,
            Evidence::Blocked => self.block += 1,
            Evidence::ConnectError | Evidence::Error => self.err += 1,
        }
        self.results.insert(target.to_string(), evidence);
    }
}

impl Display for Counter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total = self.total();
        write!(f, "OK {} ({:.2}%) | Blocked {} (early: {}) ({:.2}%) | Error {} ({:.2}%)",
               self.ok,
               self.ok as f32 / total as f32 * 100.0,
               self.block, self.early,
               self.block as f32 / total as f32 * 100.0,
               self.err,
               self.err as f32 / total as f32 * 100.0)
    }
}

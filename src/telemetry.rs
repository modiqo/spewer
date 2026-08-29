//! Traceable cost derivation and comparable-run exports.

use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Receipt, Usage};
use crate::util::sha256;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Per-million-token provider rates in US dollars.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelPrice {
    /// Uncached input-token rate.
    pub input_per_million: f64,
    /// Cached input-token rate.
    pub cached_input_per_million: f64,
    /// Output-token rate.
    pub output_per_million: f64,
    /// Reasoning-token rate, when billed separately.
    pub reasoning_per_million: f64,
}

/// Immutable, source-labelled price configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PriceConfig {
    /// Configuration format version.
    pub version: u32,
    /// Human-readable origin of the rates.
    pub source: String,
    /// RFC 3339 effective time.
    pub effective_at: String,
    /// Rates keyed by exact observed model.
    pub models: BTreeMap<String, ModelPrice>,
}

impl PriceConfig {
    /// Returns the hash stored beside every derived cost.
    pub fn hash(&self) -> Result<String> {
        sha256(&serde_json::to_vec(self)?)
    }

    /// Derives cost without replacing missing provider facts with zeros.
    pub fn price(&self, model: &str, usage: &mut Usage) -> Result<()> {
        let rate = self
            .models
            .get(model)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "model has no price entry"))?;
        let input = usage
            .input_tokens
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "input usage is missing"))?;
        let output = usage
            .output_tokens
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "output usage is missing"))?;
        let cached = usage.cached_input_tokens.map_or(0, |value| value);
        let reasoning = usage.reasoning_tokens.map_or(0, |value| value);
        let cost = dollars(input, rate.input_per_million)?
            + dollars(cached, rate.cached_input_per_million)?
            + dollars(output, rate.output_per_million)?
            + dollars(reasoning, rate.reasoning_per_million)?;
        if !cost.is_finite() || cost < 0.0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "derived cost is invalid",
            ));
        }
        usage.actual_cost_usd = Some(cost);
        usage.price_config_hash = Some(self.hash()?);
        Ok(())
    }
}

/// Applies an explicitly configured price file to a terminal receipt.
pub async fn price_from_environment(receipt: &mut Receipt) -> Result<()> {
    let Some(path) = std::env::var_os("SPEWER_PRICE_CONFIG") else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let json = tokio::task::spawn_blocking(move || std::fs::read_to_string(path)).await??;
    let config: PriceConfig = serde_json::from_str(&json)?;
    let model = match receipt.engine.observed_models.last() {
        Some(model) => model.clone(),
        None => receipt.engine.requested_model.clone(),
    };
    config.price(&model, &mut receipt.usage)
}

/// Machine-readable quality and cost observation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunExport {
    /// Comparable workload class chosen before comparison.
    pub task_class: String,
    /// Terminal receipt containing traceable inputs.
    pub receipt: Receipt,
    /// Passed acceptance checks.
    pub checks_passed: u64,
    /// Attempted acceptance checks.
    pub checks_attempted: u64,
}

/// One traceable point for a quality-versus-cost plot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ParetoPoint {
    /// Actual observed model.
    pub model: String,
    /// Derived provider charge.
    pub cost_usd: f64,
    /// Passed checks retained as a numerator.
    pub checks_passed: u64,
    /// Attempted checks retained as a denominator.
    pub checks_attempted: u64,
    /// Price configuration used for the x coordinate.
    pub price_config_hash: String,
}

impl RunExport {
    /// Produces a compact summary without inventing a completion percentage.
    pub fn summary(&self) -> String {
        let cost = self
            .receipt
            .usage
            .actual_cost_usd
            .map_or_else(|| "unknown".to_owned(), |value| format!("${value:.6}"));
        format!(
            "{}: {}/{} checks passed; cost {}; wall {} ms; tools {}",
            self.task_class,
            self.checks_passed,
            self.checks_attempted,
            cost,
            self.receipt.usage.wall_ms,
            self.receipt.usage.tool_calls
        )
    }
}

/// Validates that two Pareto observations represent the same task class.
pub fn comparable(left: &RunExport, right: &RunExport, override_class: bool) -> Result<()> {
    if left.task_class != right.task_class && !override_class {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Pareto comparison requires the same task class",
        ));
    }
    Ok(())
}

/// Converts comparable exports into plot-ready points without hiding denominators.
pub fn pareto_points(runs: &[RunExport], override_class: bool) -> Result<Vec<ParetoPoint>> {
    if let Some(first) = runs.first() {
        for run in runs.iter().skip(1) {
            comparable(first, run, override_class)?;
        }
    }
    runs.iter()
        .map(|run| {
            let cost_usd = run.receipt.usage.actual_cost_usd.ok_or_else(|| {
                Error::new(ErrorKind::InvalidInput, "Pareto point has no derived cost")
            })?;
            let price_config_hash =
                run.receipt.usage.price_config_hash.clone().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidInput, "Pareto point has no price hash")
                })?;
            let model = match run.receipt.engine.observed_models.last() {
                Some(model) => model.clone(),
                None => run.receipt.engine.requested_model.clone(),
            };
            Ok(ParetoPoint {
                model,
                cost_usd,
                checks_passed: run.checks_passed,
                checks_attempted: run.checks_attempted,
                price_config_hash,
            })
        })
        .collect()
}

fn dollars(tokens: u64, per_million: f64) -> Result<f64> {
    let bounded = u32::try_from(tokens)
        .map_err(|_| Error::new(ErrorKind::InvalidInput, "token count exceeds price range"))?;
    Ok((f64::from(bounded) / 1_000_000.0) * per_million)
}

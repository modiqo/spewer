//! Deterministic hard-budget evaluation.

use crate::protocol::{Budgets, Usage};
use serde::{Deserialize, Serialize};

/// First policy boundary exceeded by an attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetBreach {
    /// Wall-clock deadline reached.
    Wall,
    /// Provider-reported tokens exceeded the limit.
    Tokens,
    /// Observed tool calls exceeded the limit.
    ToolCalls,
    /// Retry count exceeded the limit.
    Retries,
    /// Derived provider charge exceeded the limit.
    Cost,
}

/// Returns the first exceeded boundary in fixed policy order.
pub fn evaluate(
    limits: &Budgets,
    usage: &Usage,
    elapsed_ms: u64,
    retries: u32,
) -> Option<BudgetBreach> {
    if elapsed_ms >= limits.wall_seconds.saturating_mul(1_000) {
        return Some(BudgetBreach::Wall);
    }
    if token_total(usage).is_some_and(|total| total > limits.tokens) {
        return Some(BudgetBreach::Tokens);
    }
    if usage.tool_calls > limits.tool_calls {
        return Some(BudgetBreach::ToolCalls);
    }
    if retries > limits.retries {
        return Some(BudgetBreach::Retries);
    }
    if usage
        .actual_cost_usd
        .is_some_and(|cost| cost > limits.cost_usd)
    {
        return Some(BudgetBreach::Cost);
    }
    None
}

fn token_total(usage: &Usage) -> Option<u64> {
    [
        usage.input_tokens,
        usage.output_tokens,
        usage.reasoning_tokens,
    ]
    .into_iter()
    .flatten()
    .try_fold(0_u64, u64::checked_add)
}

#[cfg(test)]
mod tests {
    use super::{BudgetBreach, evaluate};
    use crate::protocol::{Budgets, Usage};

    #[test]
    fn boundaries_are_explicit_and_missing_usage_is_unknown() {
        let limits = Budgets {
            wall_seconds: 10,
            tokens: 100,
            tool_calls: 5,
            retries: 1,
            cost_usd: 1.0,
        };
        assert_eq!(evaluate(&limits, &Usage::default(), 9_999, 0), None);
        assert_eq!(
            evaluate(&limits, &Usage::default(), 10_000, 0),
            Some(BudgetBreach::Wall)
        );
        let usage = Usage {
            input_tokens: Some(80),
            output_tokens: Some(21),
            ..Usage::default()
        };
        assert_eq!(evaluate(&limits, &usage, 0, 0), Some(BudgetBreach::Tokens));
    }
}

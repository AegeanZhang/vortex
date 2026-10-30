use std::{collections::BTreeMap, error::Error, fmt};

use crate::{ToolCall, ToolRegistry};

pub const DEFAULT_TOOL_CALL_LIMIT_PER_TURN: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallLimits {
    per_tool: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallLimitsError;

impl fmt::Display for ToolCallLimitsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("工具名称必须为 1-128 个 ASCII 字母、数字、下划线或连字符，次数必须大于零")
    }
}

impl Error for ToolCallLimitsError {}

impl Default for ToolCallLimits {
    fn default() -> Self {
        Self {
            per_tool: BTreeMap::from([("web_search".into(), 4)]),
        }
    }
}

impl ToolCallLimits {
    pub fn new(per_tool: BTreeMap<String, u32>) -> Result<Self, ToolCallLimitsError> {
        let valid = per_tool.iter().all(|(name, limit)| {
            !name.is_empty()
                && name.len() <= 128
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                && *limit > 0
        });
        if !valid {
            return Err(ToolCallLimitsError);
        }
        Ok(Self { per_tool })
    }

    pub fn overrides(&self) -> &BTreeMap<String, u32> {
        &self.per_tool
    }

    pub fn limit_for(&self, name: &str) -> u32 {
        self.per_tool
            .get(name)
            .copied()
            .unwrap_or(DEFAULT_TOOL_CALL_LIMIT_PER_TURN)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolBudgetExceeded {
    pub tool_name: String,
    pub limit: u32,
    pub used: u32,
    pub requested: u64,
}

impl fmt::Display for ToolBudgetExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tool call budget exceeded: {} (limit={}, used={}, requested={})",
            self.tool_name, self.limit, self.used, self.requested
        )
    }
}

impl Error for ToolBudgetExceeded {}

pub(crate) struct TurnToolBudget {
    limits: ToolCallLimits,
    used: BTreeMap<String, u32>,
}

impl TurnToolBudget {
    pub(crate) fn new(limits: ToolCallLimits) -> Self {
        Self {
            limits,
            used: BTreeMap::new(),
        }
    }

    fn check(&self, name: &str, requested: u64) -> Result<(), ToolBudgetExceeded> {
        let limit = self.limits.limit_for(name);
        let used = self.used.get(name).copied().unwrap_or(0);
        if requested > u64::from(limit.saturating_sub(used)) {
            return Err(ToolBudgetExceeded {
                tool_name: name.into(),
                limit,
                used,
                requested,
            });
        }
        Ok(())
    }

    pub(crate) fn preflight(
        &self,
        calls: &[ToolCall],
        tools: &ToolRegistry,
    ) -> Result<(), ToolBudgetExceeded> {
        let mut requested = BTreeMap::<&str, u64>::new();
        for call in calls {
            if tools.contains(&call.name) {
                let count = requested.entry(&call.name).or_default();
                *count = count.saturating_add(1);
            }
        }
        for (name, count) in requested {
            self.check(name, count)?;
        }
        Ok(())
    }

    pub(crate) fn consume(&mut self, name: &str) -> Result<(), ToolBudgetExceeded> {
        self.check(name, 1)?;
        // check 已保证 used < limit <= u32::MAX。
        *self.used.entry(name.into()).or_default() += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolFuture, ToolSpec};
    use std::sync::Arc;

    struct StubTool(&'static str);
    impl Tool for StubTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.0.into(),
                description: "fixture".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }
        }
        fn execute(&self, _: serde_json::Value) -> ToolFuture<'_> {
            Box::pin(async { Ok("fixture".into()) })
        }
    }

    fn call(name: &str, id: usize) -> ToolCall {
        ToolCall {
            call_id: id.to_string(),
            name: name.into(),
            arguments: serde_json::json!({}),
        }
    }

    #[test]
    fn explicit_limits_override_fallback_without_a_fifty_ceiling() {
        let limits = ToolCallLimits::new(BTreeMap::from([("local".into(), 80)])).unwrap();
        assert_eq!(limits.limit_for("local"), 80);
        assert_eq!(limits.limit_for("other"), 50);
        assert_eq!(ToolCallLimits::default().limit_for("web_search"), 4);
        assert!(ToolCallLimits::new(BTreeMap::from([("local".into(), 0)])).is_err());
        assert!(ToolCallLimits::new(BTreeMap::from([("bad name".into(), 2)])).is_err());
    }

    #[test]
    fn fallback_allows_fifty_rejects_fifty_first_and_resets_for_a_new_turn() {
        let limits = ToolCallLimits::default();
        let mut budget = TurnToolBudget::new(limits.clone());
        for _ in 0..50 {
            budget.consume("local").unwrap();
        }
        assert_eq!(budget.consume("local").unwrap_err().used, 50);
        assert!(budget.consume("other").is_ok());
        assert!(TurnToolBudget::new(limits).consume("local").is_ok());
    }

    #[test]
    fn preflight_aggregates_without_charging_and_unknown_tools_are_ignored() {
        let tools = ToolRegistry::new(vec![Arc::new(StubTool("web_search"))]).unwrap();
        let limits = ToolCallLimits::new(BTreeMap::from([
            ("web_search".into(), 4),
            ("missing".into(), 1),
        ]))
        .unwrap();
        let mut budget = TurnToolBudget::new(limits);
        budget.consume("web_search").unwrap();
        budget.consume("web_search").unwrap();
        budget.consume("web_search").unwrap();
        // 即使未知工具的同名额度已耗尽，预检也应忽略它。
        budget.consume("missing").unwrap();
        let batch = [
            call("web_search", 1),
            call("web_search", 2),
            call("missing", 3),
        ];
        let error = budget.preflight(&batch, &tools).unwrap_err();
        assert_eq!((error.limit, error.used, error.requested), (4, 3, 2));
        assert_eq!(budget.used["web_search"], 3);
        budget
            .preflight(&[call("web_search", 4), call("missing", 5)], &tools)
            .unwrap();
        assert_eq!(budget.used["web_search"], 3);
        assert_eq!(budget.used["missing"], 1);
    }

    #[test]
    fn different_tools_have_independent_limits_and_preflight_is_atomic() {
        let tools = ToolRegistry::new(vec![
            Arc::new(StubTool("web_search")),
            Arc::new(StubTool("local")),
        ])
        .unwrap();
        let mut budget = TurnToolBudget::new(ToolCallLimits::default());
        for _ in 0..4 {
            budget.consume("web_search").unwrap();
        }
        budget
            .preflight(&[call("local", 1), call("local", 2)], &tools)
            .unwrap();
        let used = budget.used.clone();
        let error = budget
            .preflight(&[call("local", 3), call("web_search", 4)], &tools)
            .unwrap_err();
        assert_eq!(error.tool_name, "web_search");
        assert_eq!(budget.used, used);
        budget.consume("local").unwrap();
        assert_eq!(budget.used["local"], 1);
    }
}

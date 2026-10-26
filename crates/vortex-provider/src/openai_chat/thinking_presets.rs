use vortex_core::{ThinkingCapabilities, ThinkingCapabilitiesOverride};

use super::options::{OpenAiChatDialect, ReasoningHistoryPolicy, RequestOptionsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedThinkingProfile {
    pub capabilities: ThinkingCapabilities,
    pub history_policy: ReasoningHistoryPolicy,
}

/// 纯本地解析；不修改路由、策略或凭据，不证明远端真实支持。
pub fn resolve_thinking_profile(
    dialect: OpenAiChatDialect,
    preset: Option<&str>,
    overrides: ThinkingCapabilitiesOverride,
) -> Result<ResolvedThinkingProfile, RequestOptionsError> {
    let (base, history_policy) = match preset {
        None => (
            ThinkingCapabilities::default(),
            ReasoningHistoryPolicy::Unverified,
        ),
        Some("dashscope-qwen-display-only") => {
            if dialect != OpenAiChatDialect::DashScope {
                return Err(RequestOptionsError::ThinkingPresetDialectMismatch { dialect });
            }
            (
                ThinkingCapabilities {
                    can_enable: true,
                    can_disable: true,
                    supports_history_control: true,
                },
                ReasoningHistoryPolicy::DisplayOnlyWithHistoryDisabled,
            )
        }
        Some(_) => return Err(RequestOptionsError::UnknownThinkingPreset),
    };
    Ok(ResolvedThinkingProfile {
        capabilities: overrides.apply_to(base),
        history_policy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_preset_is_conservative() {
        let resolved =
            resolve_thinking_profile(OpenAiChatDialect::DashScope, None, Default::default())
                .unwrap();
        assert_eq!(resolved.capabilities, ThinkingCapabilities::default());
        assert_eq!(resolved.history_policy, ReasoningHistoryPolicy::Unverified);
    }

    #[test]
    fn preset_declares_capabilities_and_history_contract() {
        let resolved = resolve_thinking_profile(
            OpenAiChatDialect::DashScope,
            Some("dashscope-qwen-display-only"),
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            resolved.capabilities,
            ThinkingCapabilities {
                can_enable: true,
                can_disable: true,
                supports_history_control: true,
            }
        );
        assert_eq!(
            resolved.history_policy,
            ReasoningHistoryPolicy::DisplayOnlyWithHistoryDisabled
        );
    }

    #[test]
    fn explicit_false_overrides_each_preset_capability() {
        for overrides in [
            ThinkingCapabilitiesOverride {
                can_enable: Some(false),
                ..Default::default()
            },
            ThinkingCapabilitiesOverride {
                can_disable: Some(false),
                ..Default::default()
            },
            ThinkingCapabilitiesOverride {
                supports_history_control: Some(false),
                ..Default::default()
            },
        ] {
            let resolved = resolve_thinking_profile(
                OpenAiChatDialect::DashScope,
                Some("dashscope-qwen-display-only"),
                overrides,
            )
            .unwrap();
            assert_eq!(
                resolved.capabilities,
                ThinkingCapabilities {
                    can_enable: overrides.can_enable.unwrap_or(true),
                    can_disable: overrides.can_disable.unwrap_or(true),
                    supports_history_control: overrides.supports_history_control.unwrap_or(true),
                }
            );
            assert_eq!(
                resolved.history_policy,
                ReasoningHistoryPolicy::DisplayOnlyWithHistoryDisabled
            );
        }
    }

    #[test]
    fn capability_overrides_cannot_create_a_history_contract() {
        let resolved = resolve_thinking_profile(
            OpenAiChatDialect::DashScope,
            None,
            ThinkingCapabilitiesOverride {
                can_enable: Some(true),
                can_disable: Some(true),
                supports_history_control: Some(true),
            },
        )
        .unwrap();
        assert!(resolved.capabilities.can_enable);
        assert!(resolved.capabilities.can_disable);
        assert!(resolved.capabilities.supports_history_control);
        assert_eq!(resolved.history_policy, ReasoningHistoryPolicy::Unverified);
    }

    #[test]
    fn unknown_preset_and_mismatched_dialects_are_rejected() {
        assert_eq!(
            resolve_thinking_profile(
                OpenAiChatDialect::DashScope,
                Some("unknown-preset"),
                Default::default()
            )
            .unwrap_err(),
            RequestOptionsError::UnknownThinkingPreset
        );
        for dialect in [OpenAiChatDialect::Generic, OpenAiChatDialect::DeepSeek] {
            assert_eq!(
                resolve_thinking_profile(
                    dialect,
                    Some("dashscope-qwen-display-only"),
                    Default::default()
                )
                .unwrap_err(),
                RequestOptionsError::ThinkingPresetDialectMismatch { dialect }
            );
        }
    }

    #[test]
    fn independent_resolutions_do_not_share_preset_state() {
        let _ = resolve_thinking_profile(
            OpenAiChatDialect::DashScope,
            Some("dashscope-qwen-display-only"),
            Default::default(),
        )
        .unwrap();
        let resolved =
            resolve_thinking_profile(OpenAiChatDialect::DeepSeek, None, Default::default())
                .unwrap();
        assert_eq!(resolved.capabilities, ThinkingCapabilities::default());
        assert_eq!(resolved.history_policy, ReasoningHistoryPolicy::Unverified);
    }
}

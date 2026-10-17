#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlashCommand {
    Exit,
    Model,
}

#[derive(Debug)]
pub(crate) struct CommandSpec {
    pub(crate) command: SlashCommand,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
}

pub(crate) static COMMANDS: [CommandSpec; 2] = [
    CommandSpec {
        command: SlashCommand::Exit,
        name: "exit",
        description: "退出 Vortex (终止当前对话)",
    },
    CommandSpec {
        command: SlashCommand::Model,
        name: "model",
        description: "选择本会话使用的模型",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedCommand {
    Exit,
    Model { model_id: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandParse {
    Incomplete,
    Invalid(&'static str),
    NotCommand,
    Ready(ParsedCommand),
}

pub(crate) fn completion_prefix(text: &str) -> Option<&str> {
    let prefix = text.strip_prefix('/')?;
    if prefix
        .chars()
        .any(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
    {
        return None;
    }
    Some(prefix)
}

pub(crate) fn parse_command(text: &str) -> CommandParse {
    use vortex_core::config::valid_profile_id;

    let Some(body) = text.strip_prefix('/') else {
        return CommandParse::NotCommand;
    };

    if body.chars().any(|ch| matches!(ch, '\r' | '\n')) {
        return CommandParse::Invalid("slash 命令必须是单行文本");
    }

    if body.is_empty() {
        return CommandParse::Incomplete;
    }

    let name = body.split([' ', '\t']).next().unwrap_or_default();
    if name.is_empty() {
        return CommandParse::Invalid("/ 后必须紧接命令名");
    }

    let args = body[name.len()..].trim_matches([' ', '\t']);

    let command = COMMANDS
        .iter()
        .find(|spec| spec.name == name)
        .map(|spec| spec.command);

    match command {
        Some(SlashCommand::Exit) if args.is_empty() => CommandParse::Ready(ParsedCommand::Exit),
        Some(SlashCommand::Exit) => CommandParse::Invalid("/exit 不接受参数"),

        Some(SlashCommand::Model) if args.is_empty() => {
            CommandParse::Ready(ParsedCommand::Model { model_id: None })
        }
        Some(SlashCommand::Model) if valid_profile_id(args) => {
            CommandParse::Ready(ParsedCommand::Model {
                model_id: Some(args.into()),
            })
        }
        Some(SlashCommand::Model) => {
            CommandParse::Invalid("/model 只接受一个合法的模型 profile ID")
        }

        None if body == name && COMMANDS.iter().any(|spec| spec.name.starts_with(name)) => {
            CommandParse::Incomplete
        }

        None => CommandParse::Invalid("未知 slash 命令；输入 / 查看可用命令"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_complete_exit_with_only_trailing_spaces_or_tabs() {
        for text in ["/exit", "/exit ", "/exit\t", "/exit \t"] {
            assert_eq!(
                parse_command(text),
                CommandParse::Ready(ParsedCommand::Exit),
                "{text:?}"
            );
        }
    }

    #[test]
    fn ordinary_prompts_require_no_leading_slash() {
        for text in ["", "hello", " /exit", "\t/exit", "说明 /exit", "\n/exit"] {
            assert_eq!(parse_command(text), CommandParse::NotCommand, "{text:?}");
        }
    }

    #[test]
    fn known_prefixes_are_incomplete_commands() {
        for text in ["/", "/e", "/ex", "/exi"] {
            assert_eq!(parse_command(text), CommandParse::Incomplete, "{text:?}");
        }
    }

    #[test]
    fn rejects_unknown_names_arguments_and_multiline_commands() {
        for text in [
            "/exit now",
            "/exit\tnow",
            "/exitfoo",
            "/EX",
            "//exit",
            "/ exit",
            "/ex ",
            "/ex now",
            "/exit\n",
            "/exit\r",
            "/exit\r\n",
            "/exit\nhello",
            "/退出",
            "/exit\u{a0}",
        ] {
            assert!(
                matches!(parse_command(text), CommandParse::Invalid(_)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn completion_uses_raw_single_token_prefix() {
        for (text, prefix) in [("/", ""), ("/ex", "ex"), ("/exit", "exit"), ("/zzz", "zzz")] {
            assert_eq!(completion_prefix(text), Some(prefix));
        }
        for text in [
            "",
            " /ex",
            "说明 /ex",
            "/ex ",
            "/exit\t",
            "/exit\n",
            "/exit\r",
        ] {
            assert_eq!(completion_prefix(text), None, "{text:?}");
        }
    }
}

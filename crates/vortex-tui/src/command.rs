#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlashCommand {
    Exit,
}

#[derive(Debug)]
pub(crate) struct CommandSpec {
    pub(crate) command: SlashCommand,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
}

pub(crate) static COMMANDS: [CommandSpec; 1] = [CommandSpec {
    command: SlashCommand::Exit,
    name: "exit",
    description: "退出 Vortex (终止当前对话)",
}];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandParse {
    Incomplete,
    Invalid(&'static str),
    NotCommand,
    Ready(SlashCommand),
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
    // strip_prefix，If the string does not start with prefix, returns None.
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

    if let Some(spec) = COMMANDS.iter().find(|spec| spec.name == name) {
        // name 是从body 拆出的前缀， name.len() 必须位于 UTF-8 字符边界.
        let suffix = &body[name.len()..];
        if !suffix.trim_matches([' ', '\t']).is_empty() {
            return CommandParse::Invalid("/exit 不接受参数");
        }
        return CommandParse::Ready(spec.command);
    }

    if body == name && COMMANDS.iter().any(|spec| spec.name.starts_with(name)) {
        return CommandParse::Incomplete;
    }

    CommandParse::Invalid("未知 slash 命令; 输入 / 查看可用命令")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_complete_exit_with_only_trailing_spaces_or_tabs() {
        for text in ["/exit", "/exit ", "/exit\t", "/exit \t"] {
            assert_eq!(
                parse_command(text),
                CommandParse::Ready(SlashCommand::Exit),
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

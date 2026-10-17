use crate::command::{COMMANDS, CommandSpec, completion_prefix};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

#[derive(Debug)]
pub(crate) struct CommandMenu {
    candidates: Vec<&'static CommandSpec>,
    selected: usize,
    dismissed: bool,
}

impl Default for CommandMenu {
    fn default() -> Self {
        Self {
            candidates: Vec::new(),
            selected: 0,
            dismissed: true,
        }
    }
}

impl CommandMenu {
    pub(crate) fn sync(&mut self, text: &str) {
        self.selected = 0;
        match completion_prefix(text) {
            Some(prefix) => {
                self.candidates = COMMANDS
                    .iter()
                    .filter(|spec| spec.name.starts_with(prefix))
                    .collect();
                self.dismissed = false;
            }
            None => {
                self.candidates.clear();
                self.dismissed = true;
            }
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        !self.dismissed
    }

    pub(crate) fn dismiss(&mut self) {
        self.dismissed = true;
    }

    pub(crate) fn previous(&mut self) {
        let len = self.candidates.len();
        if len == 0 {
            return;
        }
        self.selected = if self.selected == 0 {
            len - 1
        } else {
            self.selected - 1
        };
    }

    pub(crate) fn next(&mut self) {
        if !self.candidates.is_empty() {
            self.selected = (self.selected + 1) % self.candidates.len();
        }
    }

    pub(crate) fn selected_command(&self) -> Option<&'static CommandSpec> {
        if !self.is_open() {
            return None;
        }
        self.candidates.get(self.selected).copied()
    }
    pub(crate) fn desired_height(&self) -> u16 {
        // 最多 5 个内容行，加上下边框; 无匹配也占一个内容行.
        self.candidates.len().clamp(1, 5) as u16 + 2
    }
    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        if !self.is_open() || area.width < 4 || area.height < 3 {
            return;
        }

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Commands · Tab 补全 · Esc 关闭");

        if self.candidates.is_empty() {
            frame.render_widget(Paragraph::new("没有匹配的命令").block(block), area);
            return;
        }

        let items: Vec<ListItem<'static>> = self
            .candidates
            .iter()
            .map(|spec| ListItem::new(format!("/{}  {}", spec.name, spec.description)))
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::White).bg(Color::DarkGray));
        let mut list_state = ListState::default();
        list_state.select(Some(self.selected));
        frame.render_stateful_widget(list, area, &mut list_state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_filters_and_respects_dismissal() {
        let mut menu = CommandMenu::default();
        assert!(!menu.is_open());
        assert!(menu.selected_command().is_none());

        menu.sync("/");
        assert!(menu.is_open());
        assert_eq!(
            menu.candidates
                .iter()
                .map(|spec| spec.name)
                .collect::<Vec<_>>(),
            vec!["exit", "model"]
        );
        assert_eq!(menu.selected_command().unwrap().name, "exit");
        assert_eq!(menu.desired_height(), 4);

        for text in ["/e", "/ex", "/exit"] {
            menu.sync(text);
            assert!(menu.is_open());
            assert_eq!(menu.candidates.len(), 1);
            assert_eq!(
                menu.selected_command().unwrap().command,
                crate::command::SlashCommand::Exit
            );
            assert_eq!(menu.selected_command().unwrap().name, "exit");
        }

        for text in ["/m", "/mo", "/model"] {
            menu.sync(text);
            assert!(menu.is_open());
            assert_eq!(menu.candidates.len(), 1);
            assert_eq!(
                menu.selected_command().unwrap().command,
                crate::command::SlashCommand::Model
            );
            assert_eq!(menu.selected_command().unwrap().name, "model");
        }

        menu.dismiss();
        assert!(!menu.is_open());
        assert!(menu.selected_command().is_none());
        menu.sync("/ex");
        assert!(menu.is_open());

        for text in ["ordinary", " /exit", "/exit ", "/exit\n"] {
            menu.sync(text);
            assert!(!menu.is_open());
            assert!(menu.candidates.is_empty());
        }
    }

    #[test]
    fn navigation_wraps_a_single_candidate_without_changing_it() {
        let mut menu = CommandMenu::default();
        menu.sync("/ex");
        for _ in 0..3 {
            menu.previous();
            assert_eq!(menu.selected, 0);
            assert_eq!(menu.selected_command().unwrap().name, "exit");
            menu.next();
            assert_eq!(menu.selected, 0);
            assert_eq!(menu.selected_command().unwrap().name, "exit");
        }
        assert_eq!(menu.desired_height(), 3);
    }

    #[test]
    fn navigation_wraps_between_exit_and_model() {
        let mut menu = CommandMenu::default();
        menu.sync("/");
        assert_eq!(menu.selected_command().unwrap().name, "exit");

        menu.previous();
        assert_eq!(menu.selected_command().unwrap().name, "model");
        menu.previous();
        assert_eq!(menu.selected_command().unwrap().name, "exit");
        menu.next();
        assert_eq!(menu.selected_command().unwrap().name, "model");
        menu.next();
        assert_eq!(menu.selected_command().unwrap().name, "exit");

        menu.next();
        menu.sync("/ex");
        assert_eq!(menu.selected, 0);
        assert_eq!(menu.selected_command().unwrap().name, "exit");
    }

    #[test]
    fn empty_candidates_do_not_panic_or_select_a_command() {
        let mut menu = CommandMenu::default();
        menu.sync("/zzz");
        menu.previous();
        menu.next();
        assert!(menu.is_open());
        assert!(menu.selected_command().is_none());
        assert_eq!(menu.selected, 0);
        assert_eq!(menu.desired_height(), 3);
    }
}

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders},
};
use ratatui_textarea::{Input, Key, TextArea};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuKey {
    Up,
    Down,
    Tab,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromptInput(Input);

impl From<KeyEvent> for PromptInput {
    fn from(key: KeyEvent) -> Self {
        Self(key.into())
    }
}

impl PromptInput {
    pub(crate) fn menu_key(&self) -> Option<MenuKey> {
        if self.0.ctrl || self.0.alt || self.0.shift {
            return None;
        }
        match self.0.key {
            Key::Up => Some(MenuKey::Up),
            Key::Down => Some(MenuKey::Down),
            Key::Tab => Some(MenuKey::Tab),
            Key::Esc => Some(MenuKey::Escape),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct PromptEditor {
    textarea: TextArea<'static>,
}

impl Default for PromptEditor {
    fn default() -> Self {
        let mut textarea = TextArea::default();

        textarea.set_block(Block::default().borders(Borders::ALL).title("Prompt"));
        textarea.set_placeholder_text("Type a message ...");
        textarea.set_cursor_line_style(Default::default());

        Self { textarea }
    }
}

impl PromptEditor {
    pub(crate) fn handle_input(&mut self, input: PromptInput) -> bool {
        if input.0.key == Key::Enter {
            return false;
        }

        self.textarea.input(input.0)
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(&self.textarea, area);
    }

    pub(crate) fn take_text(&mut self) -> String {
        /*
        std::mem::take(self) 会：
        1. 取走当前 PromptEditor；
        2. 用 PromptEditor::default() 替换；
        3. 同时恢复 Block、placeholder、cursor style 等配置；
        4. 将旧编辑器内容转换成 String。
        相比直接清空 lines，这种方式还会重置光标、选择和 undo history。
        */
        std::mem::take(self).textarea.into_lines().join("\n")
    }

    pub(crate) fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    pub(crate) fn replace_text(&mut self, text: &str) {
        *self = Self::default();
        self.textarea.insert_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn reading_preserves_raw_text_and_taking_resets_the_editor() {
        let mut editor = PromptEditor::default();
        editor.replace_text(" /exit\n");
        assert_eq!(editor.text(), " /exit\n");
        assert_eq!(editor.text(), " /exit\n");

        editor.replace_text("/exit");
        editor.handle_input(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE).into());
        assert_eq!(editor.text(), "/exit!");
        assert!(!editor.handle_input(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE).into()));
        assert_eq!(editor.take_text(), "/exit!");
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn only_plain_menu_keys_are_intercepted() {
        for (code, expected) in [
            (KeyCode::Up, MenuKey::Up),
            (KeyCode::Down, MenuKey::Down),
            (KeyCode::Tab, MenuKey::Tab),
            (KeyCode::Esc, MenuKey::Escape),
        ] {
            let input = PromptInput::from(KeyEvent::new(code, KeyModifiers::NONE));
            assert_eq!(input.menu_key(), Some(expected));
            for modifiers in [
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::SHIFT,
            ] {
                let input = PromptInput::from(KeyEvent::new(code, modifiers));
                assert_eq!(input.menu_key(), None);
            }
        }
        let input = PromptInput::from(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(input.menu_key(), None);
    }
}

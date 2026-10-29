use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    widgets::{Block, Borders},
};
use ratatui_textarea::{Input, Key, TextArea};
use std::cell::Cell;

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
    scroll_origin: Cell<Position>,
}

impl Default for PromptEditor {
    fn default() -> Self {
        let mut textarea = TextArea::default();

        textarea.set_block(Block::default().borders(Borders::ALL).title("Prompt"));
        textarea.set_placeholder_text("Type a message ...");
        textarea.set_cursor_line_style(Default::default());

        Self {
            textarea,
            scroll_origin: Cell::new(Position::ORIGIN),
        }
    }
}

impl PromptEditor {
    pub(crate) fn handle_input(&mut self, input: PromptInput) -> bool {
        if input.0.key == Key::Enter {
            return false;
        }

        self.textarea.input(input.0)
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect) -> Option<Position> {
        let block = self.textarea.block().expect("Prompt editor has a border");
        let inner = block.inner(area);
        if inner.is_empty() {
            frame.render_widget(block, area);
            return None;
        }

        frame.render_widget(&self.textarea, area);

        // TextArea 0.9.2 只画软件光标，不公开 viewport。当前编辑器不换行、无行号，
        // 因此同步它的最小滚动规则；字符宽度和 Tab 展开仍由 screen_cursor 计算。
        // 必须在 render 后读取，以采用 resize 后的屏幕映射。
        let cursor = self.textarea.screen_cursor();
        let cursor = Position::new(cursor.col as u16, cursor.row as u16);
        let previous = self.scroll_origin.get();
        let follow = |previous: u16, cursor: u16, length: u16| {
            previous
                .min(cursor)
                .max(cursor.saturating_add(1).saturating_sub(length))
        };
        let origin = if self.textarea.is_empty() {
            Position::ORIGIN
        } else {
            Position::new(
                follow(previous.x, cursor.x, inner.width),
                follow(previous.y, cursor.y, inner.height),
            )
        };
        self.scroll_origin.set(origin);
        Some(Position::new(
            inner.x + cursor.x - origin.x,
            inner.y + cursor.y - origin.y,
        ))
    }

    pub(crate) fn copy_selection(&mut self) -> Option<String> {
        let (start, end) = self.textarea.selection_range()?;
        if start == end {
            return None;
        }
        self.textarea.copy();
        Some(self.textarea.yank_text())
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
    use ratatui::{Terminal, backend::TestBackend, style::Modifier};

    fn key(editor: &mut PromptEditor, code: KeyCode) {
        editor.handle_input(KeyEvent::new(code, KeyModifiers::NONE).into());
    }

    fn assert_cursor(editor: &PromptEditor, area: Rect, expected: Position) {
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal
            .draw(|frame| {
                let cursor = editor.render(frame, area).unwrap();
                frame.set_cursor_position(cursor);
            })
            .unwrap();
        assert_eq!(terminal.backend().cursor_position(), expected);
        assert!(terminal.backend().cursor_visible());
        // 同时检查实际绘制的软件光标，避免硬件光标和 TextArea 的 viewport 漂移。
        assert!(
            terminal.backend().buffer()[expected]
                .modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn first_character_and_clearing_keep_placeholder_out_of_the_draft() {
        let area = Rect::new(3, 2, 30, 3);
        for (ch, width) in [('a', 1), ('你', 2)] {
            let mut editor = PromptEditor::default();
            assert_cursor(&editor, area, Position::new(4, 3));
            assert_eq!(editor.text(), "");
            assert_eq!(editor.textarea.cursor(), (0, 0));

            // 中文字符是终端提交的文本；这里不模拟 IME preedit 或候选窗。
            key(&mut editor, KeyCode::Char(ch));
            assert_eq!(editor.text(), ch.to_string());
            assert_cursor(&editor, area, Position::new(4 + width, 3));
            key(&mut editor, KeyCode::Backspace);
            assert_eq!(editor.text(), "");
            assert_cursor(&editor, area, Position::new(4, 3));

            editor.replace_text("你好");
            assert_eq!(editor.take_text(), "你好");
            assert_cursor(&editor, area, Position::new(4, 3));
        }
    }

    #[test]
    fn cursor_follows_wide_combining_and_tab_characters() {
        let area = Rect::new(3, 2, 30, 3);
        let mut editor = PromptEditor::default();
        editor.replace_text("a你e\u{301}\t好");
        assert_cursor(&editor, area, Position::new(14, 3));
        key(&mut editor, KeyCode::Left);
        assert_cursor(&editor, area, Position::new(12, 3));
        key(&mut editor, KeyCode::Home);
        assert_cursor(&editor, area, Position::new(4, 3));
        key(&mut editor, KeyCode::Right);
        key(&mut editor, KeyCode::Right);
        assert_cursor(&editor, area, Position::new(7, 3));
    }

    #[test]
    fn cursor_tracks_horizontal_scroll_and_resize() {
        let area = Rect::new(3, 2, 8, 3);
        let mut editor = PromptEditor::default();
        editor.replace_text("abcdefghijk");
        assert_cursor(&editor, area, Position::new(9, 3));
        key(&mut editor, KeyCode::Left);
        assert_cursor(&editor, area, Position::new(8, 3));
        assert_cursor(&editor, Rect::new(3, 2, 6, 3), Position::new(7, 3));
        assert_cursor(&editor, Rect::new(3, 2, 30, 3), Position::new(7, 3));
        key(&mut editor, KeyCode::Home);
        assert_cursor(&editor, area, Position::new(4, 3));
        key(&mut editor, KeyCode::End);
        assert_cursor(&editor, area, Position::new(9, 3));
        assert_eq!(editor.take_text(), "abcdefghijk");
        assert_cursor(&editor, area, Position::new(4, 3));
    }

    #[test]
    fn cursor_tracks_multiline_viewport_and_vertical_movement() {
        let mut editor = PromptEditor::default();
        editor.replace_text("first\n你好\nlast");
        let area = Rect::new(3, 2, 20, 4);
        assert_cursor(&editor, area, Position::new(8, 4));
        key(&mut editor, KeyCode::Up);
        assert_cursor(&editor, area, Position::new(8, 3));
        key(&mut editor, KeyCode::Up);
        assert_cursor(&editor, area, Position::new(8, 3));
        key(&mut editor, KeyCode::Down);
        assert_cursor(&editor, Rect::new(3, 2, 20, 3), Position::new(8, 3));
        assert_eq!(editor.text(), "first\n你好\nlast");
    }

    #[test]
    fn prompt_without_inner_cells_has_no_terminal_cursor() {
        let mut editor = PromptEditor::default();
        editor.replace_text("long input");
        for (width, height) in [(0, 0), (1, 1), (2, 3), (20, 2)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    assert_eq!(editor.render(frame, frame.area()), None);
                })
                .unwrap();
            assert!(!terminal.backend().cursor_visible());
        }
        assert_cursor(&editor, Rect::new(3, 2, 20, 3), Position::new(14, 3));
    }

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

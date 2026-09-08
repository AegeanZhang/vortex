use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders},
};
use ratatui_textarea::{Input, Key, TextArea};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromptInput(Input);

impl From<KeyEvent> for PromptInput {
    fn from(key: KeyEvent) -> Self {
        Self(key.into())
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
}

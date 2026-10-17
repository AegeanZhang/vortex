use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use vortex_core::ModelInfo;

#[derive(Debug, Default)]
pub(crate) struct ModelPicker {
    models: Vec<ModelInfo>,
    selected: usize,
    current_id: Option<String>,
    open: bool,
}

impl ModelPicker {
    pub(crate) fn open(&mut self, models: &[ModelInfo], current: Option<&ModelInfo>) {
        self.models = models.to_vec();
        self.current_id = current.map(|model| model.id.clone());
        self.selected = current
            .and_then(|current| models.iter().position(|model| model.id == current.id))
            .unwrap_or(0);
        self.open = true;
    }
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
    pub(crate) fn close(&mut self) {
        self.open = false;
    }
    pub(crate) fn selected_model(&self) -> Option<&ModelInfo> {
        if self.open {
            self.models.get(self.selected)
        } else {
            None
        }
    }
    pub(crate) fn previous(&mut self) {
        let len = self.models.len();
        if len > 0 {
            self.selected = (self.selected + len - 1) % len;
        }
    }
    pub(crate) fn next(&mut self) {
        if !self.models.is_empty() {
            self.selected = (self.selected + 1) % self.models.len();
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        if !self.open || area.width < 4 || area.height < 4 {
            return;
        }
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Models · 仅本会话 · Enter 选择 · Esc 返回")
            .title_bottom("保留历史；后续请求发给所选服务");
        if self.models.is_empty() {
            frame.render_widget(Paragraph::new("没有可用模型配置").block(block), area);
            return;
        }
        let items: Vec<ListItem<'static>> = self
            .models
            .iter()
            .map(|model| {
                let mark = if self.current_id.as_deref() == Some(model.id.as_str()) {
                    "*"
                } else {
                    " "
                };
                let reason = model.unavailable_reason.as_deref().unwrap_or("可选择");
                ListItem::new(format!(
                    "{mark} {} · {}\n  {} / {} · {reason}",
                    model.id, model.display_name, model.provider_id, model.endpoint_host,
                ))
            })
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

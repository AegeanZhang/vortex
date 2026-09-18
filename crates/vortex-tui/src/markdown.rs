//! 将单条 Assistant Markdown 正文转换为 Ratatui 展示行。
//! 
//! 不管理消息、终端、宽度或滚动；原始内容由调用方持有。

use ratatui::text::{Line, Text};

pub(crate) fn render(source: &str) -> Vec<Line<'_>> {
    let Text {
        lines,
        style,
        alignment,
    } = tui_markdown::from_str(source);

    lines
        .into_iter()
        .map(|mut line| {
            // 将 Text 的默认样式下沉到行； 具体 Line/Span 样式仍可覆盖他。
            line.style = style.patch(line.style);
            line.alignment = line.alignment.or(alignment);
            line
        })
        .collect()
}

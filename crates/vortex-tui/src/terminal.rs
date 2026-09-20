use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Result, Stdout};

// 定义终端类型别名, 方便在其他模块中使用
pub(crate) type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

pub(crate) struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;

        // 从 raw mode 生效后立即接管清理；后续初始化失败时 Drop 也能恢复终端。
        let guard = Self { active: true };

        let mut stdout = std::io::stdout();

        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        Ok(guard)
    }

    pub(crate) fn cleanup(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        disable_raw_mode()?;

        let mut stdout = std::io::stdout();
        execute!(
            stdout,
            DisableMouseCapture,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;

        self.active = false;

        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();

            let mut stdout = std::io::stdout();

            let _ = execute!(
                stdout,
                DisableMouseCapture,
                LeaveAlternateScreen,
                crossterm::cursor::Show
            );
        }
    }
}

pub(crate) fn enter() -> Result<(TuiTerminal, TerminalGuard)> {
    let guard = TerminalGuard::new()?;

    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 当前阶段没有文本输入，隐藏硬件光标可避免它停留在最后一次绘制位置。
    terminal.hide_cursor()?;

    Ok((terminal, guard))
}

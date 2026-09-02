use std::process::ExitCode;

pub fn execute(_task: String) -> ExitCode {
    eprintln!("错误：vortex exec 尚未实现");
    ExitCode::FAILURE
}

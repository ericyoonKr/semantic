mod cli;
mod emit;
mod error;
mod report;
mod source;
mod validate;
mod write;

fn main() -> std::process::ExitCode {
    cli::run()
}

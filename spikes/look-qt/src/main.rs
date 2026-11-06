mod qt;
mod shell;

fn main() {
    std::process::exit(shell::main_with_args(std::env::args().skip(1).collect()));
}

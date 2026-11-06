fn main() {
    std::process::exit(qt_shell::shell::main_with_args(std::env::args().skip(1).collect()));
}

mod cli;

fn main() {
    if let Err(error) = cli::run_from_env() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

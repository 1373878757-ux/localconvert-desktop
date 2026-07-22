fn main() {
    std::process::exit(localconvert_desktop_lib::simulated_users::run_cli(
        std::env::args().skip(1),
    ));
}

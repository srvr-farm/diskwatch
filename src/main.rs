fn main() {
    if let Err(error) = diskwatch::run() {
        eprintln!("{error:?}");
        std::process::exit(1);
    }
}

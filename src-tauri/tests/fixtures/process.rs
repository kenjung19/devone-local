fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("echo") => println!("captured-output"),
        Some("fail") => {
            eprintln!("failure-output");
            std::process::exit(7);
        }
        Some("sleep") => std::thread::sleep(std::time::Duration::from_secs(30)),
        _ => std::process::exit(2),
    }
}

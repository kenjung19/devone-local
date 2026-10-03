fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("echo") => println!("captured-output"),
        Some("fail") => {
            eprintln!("failure-output");
            std::process::exit(7);
        }
        Some("listen") => {
            use std::io::{Read, Write};
            let port = std::env::args().nth(2).unwrap().parse::<u16>().unwrap();
            let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_millis(500)))
                    .unwrap();
                let mut data = [0; 512];
                if let Ok(n) = stream.read(&mut data)
                    && data[..n].starts_with(b"POST /stop")
                {
                    println!("graceful-stop");
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                    break;
                }
            }
        }
        Some("sleep") => std::thread::sleep(std::time::Duration::from_secs(30)),
        _ => std::process::exit(2),
    }
}

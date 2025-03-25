use std::process::{Command, Stdio};
use std::thread;

fn main() {
    let binaries = vec!["proxy", "run_servers", "web_server"];

    let handles: Vec<_> = binaries.into_iter().map(|binary| {
        thread::spawn(move || {
            let status = Command::new("cargo")
                .arg("run")
                .arg("--bin")
                .arg(binary)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())  
                .status(); 

            match status {
                Ok(status) => {
                    if status.success() {
                        println!("{} ran successfully.", binary);
                    } else {
                        eprintln!("{} failed to run.", binary);
                    }
                }
                Err(e) => {
                    eprintln!("Error running {}: {}", binary, e);
                }
            }
        })
    }).collect();
    
    for handle in handles {
        handle.join().unwrap();
    }
}
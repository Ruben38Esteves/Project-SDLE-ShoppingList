use std::process::{Command, Stdio};
use std::thread;
use std::io::{self, BufRead};

fn main() {
    let mut handles = vec![];

    for i in 0..6 {
        let handle = thread::spawn(move || {
            let arg = i.to_string();
            let mut child = Command::new("cargo")
                .arg("run")
                .arg("--bin")
                .arg("server")
                .arg(&arg)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to execute command");

            let stdout = child.stdout.take().expect("Failed to capture stdout");
            let stderr = child.stderr.take().expect("Failed to capture stderr");

            let stdout_handle = thread::spawn(move || {
                let reader = io::BufReader::new(stdout);
                for line in reader.lines() {
                    println!("Server {} stdout: {}", i, line.expect("Could not read line"));
                }
            });

            let stderr_handle = thread::spawn(move || {
                let reader = io::BufReader::new(stderr);
                for line in reader.lines() {
                    eprintln!("Server {} stderr: {}", i, line.expect("Could not read line"));
                }
            });

            stdout_handle.join().expect("Failed to join stdout handle");
            stderr_handle.join().expect("Failed to join stderr handle");

            child.wait().expect("Child process wasn't running");
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}
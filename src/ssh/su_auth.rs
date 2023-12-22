use std::{
    io::Write,
    process::{Command, Stdio},
};

pub fn su_login(user: &str, password: &str) -> bool {
    let mut process = Command::new("su")
        .arg(user)
        .arg("-c")
        .arg("exit 101")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to execute command");

    let mut stdin = process.stdin.take().expect("Failed to open stdin");
    stdin
        .write_all((password.to_owned() + "\n").as_bytes())
        .expect("Failed to write to stdin");
    stdin.flush().expect("Failed to flush stdin");

    let output = process.wait_with_output().expect("Failed to read stdout");

    // verify the exit code is right (if the command has indeed been existed)
    if let Some(code) = output.status.code() {
        if code == 101 {
            // just to make sure, verify the output for anything not normal
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            return stdout.is_empty() && stderr == "Password: ";
        }
    }

    false
}

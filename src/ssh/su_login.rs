use std::{
    io::Write,
    process::{Command, Stdio},
};

use anyhow::Context;

pub fn su_login(user: &str, password: &str) -> anyhow::Result<bool> {
    let mut process = if users::get_current_uid() == 0 {
        Command::new("su")
            .arg("nobody")
            .arg("-s")
            .arg("/bin/sh")
            .arg("-c")
            .arg("su -c 'exit 101'")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    } else {
        Command::new("su")
            .arg(user)
            .arg("-c")
            .arg("exit 101")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    };

    let mut stdin = process
        .stdin
        .take()
        .context("Could not get su process stdin")?;
    stdin.write_all((password.to_owned() + "\n").as_bytes())?;
    stdin.flush()?;

    let output = process.wait_with_output()?;

    // verify the exit code is right (if the command has indeed been existed)
    if let Some(code) = output.status.code() {
        if code == 101 {
            // just to make sure, verify the output for anything not normal
            let stdout = String::from_utf8(output.stdout)?;
            let stderr = String::from_utf8(output.stderr)?;
            return Ok(stdout.is_empty() && stderr == "Password: ");
        }
    }

    Ok(false)
}

use serde_json::json;
use std::process::{self, Command};

fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "open".into());
    let result = Command::new("omarchy-shell")
        .args([
            "shell",
            "summon",
            "cordrogue.omatate",
            &json!({"cmd": command}).to_string(),
        ])
        .output();
    match result {
        Ok(output)
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == "ok" => {}
        result => {
            let detail = match result {
                Ok(output) => format!(
                    "{} {}",
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                )
                .trim()
                .to_owned(),
                Err(error) => error.to_string(),
            };
            eprintln!(
                "omatate-panel: cannot summon Omatate: {detail}. Install and enable cordrogue.omatate in Omarchy Shell."
            );
            process::exit(1);
        }
    }
}

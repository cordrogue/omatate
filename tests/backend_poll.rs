use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

#[test]
fn backend_poll_detects_changes_and_caches_mutation_snapshots() {
    let root = std::env::temp_dir().join(format!("ui-notes-backend-poll-{}", std::process::id()));
    let home = root.join("home");
    let runtime = root.join("runtime");
    let state = root.join("state");
    let config = home.join(".config");
    let bin = root.join("bin");
    let session = root.join("project");
    for path in [
        home.clone(),
        runtime.join("ui-notes"),
        state.clone(),
        config.clone(),
        bin.clone(),
        session.join(".data"),
    ] {
        fs::create_dir_all(path).unwrap();
    }
    for name in ["omarchy", "ghostty"] {
        let path = bin.join(name);
        fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let entries = session.join(".data/entries.jsonl");
    let first =
        json!({"id":1,"ts":"2026-09-07T12:00:00","transcript":"first","status":"done","ai":false});
    let second =
        json!({"id":2,"ts":"2026-09-07T12:00:01","transcript":"second","status":"done","ai":false});
    fs::write(&entries, format!("{first}\n")).unwrap();
    fs::write(session.join(".data/id"), "abc").unwrap();
    fs::write(
        runtime.join("ui-notes/session"),
        format!("{}\n", session.display()),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ui-notes"))
        .arg("backend")
        .env("HOME", &home)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("XDG_STATE_HOME", &state)
        .env("XDG_CONFIG_HOME", &config)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env_remove("UI_NOTES_SESSION_PATH")
        .env_remove("UI_NOTES_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut request = |value: Value| {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], value["id"]);
        assert_eq!(reply["ok"], true, "{reply}");
        reply
    };
    let first_poll = request(json!({"id":1,"cmd":"snapshot","poll":true}));
    assert_eq!(first_poll["state"]["entries"], json!([first]));
    let unchanged = request(json!({"id":2,"cmd":"snapshot","poll":true}));
    assert_eq!(unchanged["unchanged"], true);
    assert!(unchanged.get("state").is_none());

    let temporary = entries.with_extension("jsonl.tmp");
    fs::write(&temporary, format!("{first}\n{second}\n")).unwrap();
    fs::rename(temporary, &entries).unwrap();
    let changed = request(json!({"id":3,"cmd":"snapshot","poll":true}));
    assert_eq!(changed["state"]["entries"], json!([first, second]));
    let note = request(json!({"id":4,"cmd":"note","session":session,"token":"abc","text":"third"}));
    assert_eq!(note["state"]["entries"].as_array().unwrap().len(), 3);
    let unchanged = request(json!({"id":5,"cmd":"snapshot","poll":true}));
    assert_eq!(unchanged["unchanged"], true);
    assert!(unchanged.get("state").is_none());
    for id in [6, 7] {
        let snapshot = request(json!({"id":id,"cmd":"snapshot"}));
        assert_eq!(snapshot["state"]["entries"].as_array().unwrap().len(), 3);
        assert!(snapshot.get("unchanged").is_none());
    }
    drop(input);
    assert!(child.wait().unwrap().success());
    fs::remove_dir_all(root).unwrap();
}

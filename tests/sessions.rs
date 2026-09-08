use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    runtime: PathBuf,
    state: PathBuf,
    bin: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "ui-notes-rust-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let home = root.join("home");
        let runtime = root.join("runtime/ui-notes");
        let state = root.join("state");
        let bin = root.join("bin");
        for p in [&home, &runtime, &bin] {
            fs::create_dir_all(p).unwrap()
        }
        script(&bin.join("ui-notes-panel"), "#!/bin/sh\nexit 0\n");
        script(&bin.join("omarchy-shell"), "#!/bin/sh\necho ok\n");
        script(&bin.join("omarchy"), "#!/bin/sh\nexit 1\n");
        script(&bin.join("ghostty"), "#!/bin/sh\nexit 1\n");
        Self {
            root,
            home,
            runtime,
            state,
            bin,
        }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ui-notes"))
            .args(args)
            .env("HOME", &self.home)
            .env("XDG_RUNTIME_DIR", self.root.join("runtime"))
            .env("XDG_STATE_HOME", &self.state)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env_remove("UI_NOTES_SESSION_PATH")
            .env_remove("UI_NOTES_SESSION_ID")
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Output {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&o.stderr)
        );
        o
    }
    fn pointer(&self) -> PathBuf {
        self.runtime.join("session")
    }
    fn make_session(&self, name: &str, draft: Option<&[u8]>, active: bool) -> PathBuf {
        let s = self.home.join("Documents/ui-notes").join(name);
        for p in [
            s.join(".data/transcripts"),
            s.join(".data/context"),
            s.join(".data/logs"),
        ] {
            fs::create_dir_all(p).unwrap()
        }
        fs::write(s.join(".data/entries.jsonl"), "").unwrap();
        fs::write(s.join(".data/id"), "test-session-id\n").unwrap();
        fs::write(s.join("notes.md"), "Keep this Markdown exactly.\n").unwrap();
        if let Some(d) = draft {
            fs::write(s.join(".data/draft.txt"), d).unwrap()
        }
        if active {
            fs::write(self.pointer(), format!("{}\n", s.display())).unwrap()
        }
        s
    }
    fn panel<F>(&self, handler: F) -> thread::JoinHandle<()>
    where
        F: Fn(Value) -> Value + Send + 'static,
    {
        let sock = self.runtime.join("panel.sock");
        let listener = UnixListener::bind(sock).unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            writeln!(stream, "{}", handler(serde_json::from_str(&line).unwrap())).unwrap()
        })
    }
    fn panel_commands(&self, count: usize) -> thread::JoinHandle<Vec<String>> {
        let sock = self.runtime.join("panel.sock");
        let _ = fs::remove_file(&sock);
        let listener = UnixListener::bind(sock).unwrap();
        thread::spawn(move || {
            let mut commands = Vec::new();
            for _ in 0..count {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                let value: Value = serde_json::from_str(&line).unwrap();
                commands.push(value["cmd"].as_str().unwrap().to_owned());
                writeln!(stream, "{}", json!({"ok":true})).unwrap();
            }
            commands
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    let mut p = fs::metadata(path).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(path, p).unwrap()
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn registry(f: &Fixture) -> Vec<String> {
    serde_json::from_slice(&fs::read(f.state.join("ui-notes/projects.json")).unwrap()).unwrap()
}

fn write_registry(f: &Fixture, projects: &[&Path]) {
    fs::create_dir_all(f.state.join("ui-notes")).unwrap();
    fs::write(
        f.state.join("ui-notes/projects.json"),
        serde_json::to_vec(
            &projects
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn open_default_and_open_use_selector_without_session() {
    for args in [&[][..], &["open"][..]] {
        let f = Fixture::new();
        let h = f.panel(|v| json!({"ok":v["cmd"]=="open"}));
        f.ok(args);
        h.join().unwrap();
        assert!(!f.pointer().exists());
        assert!(!f.home.join("Documents").exists())
    }
}

#[test]
fn open_default_and_open_restore_the_last_project_after_runtime_loss() {
    for args in [&[][..], &["open"][..]] {
        let f = Fixture::new();
        let older = f.make_session("older", Some(b"Older draft"), false);
        let last = f.make_session("last", Some(b"Last draft"), false);
        write_registry(&f, &[&last, &older]);

        f.ok(args);

        assert_eq!(
            fs::read_to_string(f.pointer()).unwrap().trim(),
            last.to_str().unwrap()
        );
        assert_eq!(
            fs::read(last.join(".data/draft.txt")).unwrap(),
            b"Last draft"
        );
    }
}

#[test]
fn open_restores_the_project_remembered_before_stop() {
    let f = Fixture::new();
    let project = f.root.join("project");
    fs::create_dir(&project).unwrap();
    f.ok(&["open-project", project.to_str().unwrap()]);
    fs::write(project.join(".data/draft.txt"), "unfinished note").unwrap();
    f.ok(&["stop"]);
    let before = tree(&project);

    f.ok(&["open"]);

    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        project.to_str().unwrap()
    );
    assert_eq!(tree(&project), before);
}

#[test]
fn open_keeps_the_active_project_ahead_of_the_registry() {
    let f = Fixture::new();
    let active = f.make_session("active", Some(b"Active draft"), true);
    let last = f.make_session("last", Some(b"Last draft"), false);
    write_registry(&f, &[&last]);
    let h = f.panel(|v| json!({"ok":v["cmd"]=="open"}));

    f.ok(&["open"]);
    h.join().unwrap();

    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        active.to_str().unwrap()
    );
    assert_eq!(
        fs::read(active.join(".data/draft.txt")).unwrap(),
        b"Active draft"
    );
}

#[test]
fn open_falls_back_to_the_chooser_when_the_last_project_or_registry_is_invalid() {
    for case in 0..4 {
        let f = Fixture::new();
        fs::create_dir_all(f.state.join("ui-notes")).unwrap();
        let registry = f.state.join("ui-notes/projects.json");
        let older = f.make_session("older", Some(b"Do not open"), false);
        match case {
            0 => fs::write(&registry, "broken JSON").unwrap(),
            1 => write_registry(&f, &[&f.root.join("missing"), &older]),
            2 => {
                let invalid = f.make_session("missing-notes", None, false);
                fs::remove_file(invalid.join("notes.md")).unwrap();
                write_registry(&f, &[&invalid, &older]);
            }
            3 => {
                let invalid = f.make_session("bad-draft", Some(&[0xff]), false);
                write_registry(&f, &[&invalid, &older]);
            }
            _ => unreachable!(),
        }

        f.ok(&["open"]);

        assert!(!f.pointer().exists());
        assert_eq!(
            fs::read(older.join(".data/draft.txt")).unwrap(),
            b"Do not open"
        );
    }
}

#[test]
fn toggle_and_focus_toggle_restore_the_last_project_when_launching() {
    for command in ["toggle", "focus-toggle"] {
        let f = Fixture::new();
        let last = f.make_session("last", Some(b"Keep draft"), false);
        write_registry(&f, &[&last]);

        f.ok(&[command]);

        assert_eq!(
            fs::read_to_string(f.pointer()).unwrap().trim(),
            last.to_str().unwrap()
        );
        assert_eq!(
            fs::read(last.join(".data/draft.txt")).unwrap(),
            b"Keep draft"
        );
    }
}

#[test]
fn toggle_closes_inactive_panel_and_stops_active_session() {
    let f = Fixture::new();
    let h = f.panel(|v| json!({"ok":v["cmd"]=="quit"}));
    f.ok(&["toggle"]);
    h.join().unwrap();
    assert!(!f.pointer().exists());
    assert!(!f.home.join("Documents").exists());

    let f = Fixture::new();
    let h = f.panel(|_| json!({"ok":false,"error":"draft could not be saved"}));
    let o = f.run(&["toggle"]);
    h.join().unwrap();
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("draft could not be saved"));

    let f = Fixture::new();
    let s = f.make_session("project", Some(b"Keep my draft"), true);
    let h = f.panel(|v| json!({"ok":v["cmd"]=="quit"}));
    f.ok(&["toggle"]);
    h.join().unwrap();
    assert!(!f.pointer().exists());
    assert!(s.exists());
}

#[test]
fn open_preserves_active_session_and_draft() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-test", Some(b"Keep my draft"), true);
    let h = f.panel(|v| json!({"ok":v["cmd"]=="open"}));
    f.ok(&["open"]);
    h.join().unwrap();
    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        s.to_str().unwrap()
    );
    assert_eq!(
        fs::read(s.join(".data/draft.txt")).unwrap(),
        b"Keep my draft"
    )
}

#[test]
fn focus_toggle_forwards_without_changing_the_session() {
    let f = Fixture::new();
    let s = f.make_session("project", Some(b"Keep my draft"), true);
    let h = f.panel(|v| json!({"ok":v["cmd"]=="toggle-focus"}));
    f.ok(&["focus-toggle"]);
    h.join().unwrap();
    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        s.to_str().unwrap()
    );
    assert_eq!(
        fs::read(s.join(".data/draft.txt")).unwrap(),
        b"Keep my draft"
    );

    let f = Fixture::new();
    let s = f.make_session("rejected", Some(b"Still here"), true);
    let h = f.panel(|v| {
        assert_eq!(v["cmd"], "toggle-focus");
        json!({"ok":false,"error":"draft could not be saved"})
    });
    let output = f.run(&["focus-toggle"]);
    h.join().unwrap();
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("draft could not be saved"));
    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        s.to_str().unwrap()
    );
    assert_eq!(fs::read(s.join(".data/draft.txt")).unwrap(), b"Still here");

    let f = Fixture::new();
    let last = f.make_session("last", Some(b"Keep waiting"), false);
    write_registry(&f, &[&last]);
    let h = f.panel(|v| json!({"ok":v["cmd"]=="toggle-focus"}));
    f.ok(&["focus-toggle"]);
    h.join().unwrap();
    assert!(!f.pointer().exists());
}

#[test]
fn open_project_creates_layout_preserves_files_and_resumes_byte_exact() {
    let f = Fixture::new();
    let p = f.root.join("project with spaces");
    fs::create_dir(&p).unwrap();
    fs::write(p.join("source.py"), "untouched").unwrap();
    f.ok(&["open-project", p.to_str().unwrap()]);
    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        p.to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(p.join("source.py")).unwrap(),
        "untouched"
    );
    for n in ["transcripts", "context", "logs"] {
        assert!(p.join(".data").join(n).is_dir())
    }
    f.ok(&["stop"]);
    let before = tree(&p);
    f.ok(&["open-project", p.to_str().unwrap()]);
    assert_eq!(before, tree(&p))
}

#[test]
fn open_project_rejects_collisions_bad_draft_and_active_race() {
    for name in ["notes.md", ".data"] {
        let f = Fixture::new();
        let p = f.root.join("p");
        fs::create_dir(&p).unwrap();
        fs::write(p.join(name), "user").unwrap();
        let o = f.run(&["open-project", p.to_str().unwrap()]);
        assert!(!o.status.success());
        assert!(text(&o.stderr).contains("cannot open project"));
        assert_eq!(fs::read_to_string(p.join(name)).unwrap(), "user")
    }
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, false);
    fs::write(s.join(".data/draft.txt"), [0xff]).unwrap();
    assert!(
        !f.run(&["open-project", s.to_str().unwrap()])
            .status
            .success()
    );
    assert!(!f.pointer().exists());
    let f = Fixture::new();
    let current = f.make_session("20260904-120000-x", None, true);
    let p = f.root.join("other");
    fs::create_dir(&p).unwrap();
    let o = f.run(&["open-project", p.to_str().unwrap()]);
    assert!(text(&o.stderr).contains("another session is active"));
    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        current.to_str().unwrap()
    );
    assert_eq!(fs::read_dir(p).unwrap().count(), 0)
}

#[test]
fn open_project_rolls_back_when_pointer_install_fails() {
    let f = Fixture::new();
    let p = f.root.join("project");
    fs::create_dir(&p).unwrap();
    fs::create_dir(f.runtime.join("session.tmp")).unwrap();
    assert!(
        !f.run(&["open-project", p.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(fs::read_dir(p).unwrap().count(), 0)
}

#[test]
fn projects_merge_registry_active_and_legacy_deduplicated() {
    let f = Fixture::new();
    let legacy = f.make_session("20260904-120000-legacy", None, false);
    let p = f.root.join("external");
    fs::create_dir(&p).unwrap();
    f.ok(&["open-project", p.to_str().unwrap()]);
    f.ok(&["stop"]);
    let alias = f.root.join("alias");
    std::os::unix::fs::symlink(&p, &alias).unwrap();
    let reg = f.state.join("ui-notes/projects.json");
    fs::write(
        &reg,
        serde_json::to_vec(&json!([alias, legacy, f.root.join("missing")])).unwrap(),
    )
    .unwrap();
    let out = f.ok(&["projects"]);
    let got: Vec<String> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        got,
        vec![
            p.canonicalize().unwrap().to_string_lossy(),
            legacy.canonicalize().unwrap().to_string_lossy()
        ]
    )
}

#[test]
fn malformed_registry_is_preserved_and_warned() {
    let f = Fixture::new();
    fs::create_dir_all(f.state.join("ui-notes")).unwrap();
    let reg = f.state.join("ui-notes/projects.json");
    fs::write(&reg, "broken JSON").unwrap();
    let p = f.root.join("new");
    fs::create_dir(&p).unwrap();
    let o = f.ok(&["open-project", p.to_str().unwrap()]);
    assert!(text(&o.stderr).contains("cannot remember project"));
    assert_eq!(fs::read_to_string(&reg).unwrap(), "broken JSON");
    let o = f.ok(&["projects"]);
    assert!(text(&o.stderr).contains("cannot read project list"));
    assert_eq!(fs::read_to_string(&reg).unwrap(), "broken JSON")
}

#[test]
fn select_project_switches_renders_preserves_drafts_and_registry() {
    let f = Fixture::new();
    let current = f.make_session("20260904-120000-current", Some(b"Outgoing"), true);
    fs::write(
        current.join(".data/entries.jsonl"),
        format!(
            "{}\n",
            json!({"id":1,"ai":false,"status":"done","transcript":"Edited"})
        ),
    )
    .unwrap();
    let target = f.make_session("20260904-140000-target", Some(b"Target"), false);
    let before = tree(&target);
    f.ok(&[
        "select-project",
        target.to_str().unwrap(),
        current.to_str().unwrap(),
    ]);
    assert!(
        fs::read_to_string(current.join("notes.md"))
            .unwrap()
            .contains("Edited")
    );
    assert_eq!(tree(&target), before);
    assert_eq!(
        &registry(&f)[..2],
        &[target.to_string_lossy(), current.to_string_lossy()]
    )
}

#[test]
fn select_project_rejects_stale_busy_and_render_failure_without_target_changes() {
    for status in [None, Some("recording"), Some("transcribing")] {
        let f = Fixture::new();
        let current = f.make_session("20260904-120000-current", None, true);
        let p = f.root.join("target");
        fs::create_dir(&p).unwrap();
        let expected = if let Some(status) = status {
            fs::write(
                current.join(".data/entries.jsonl"),
                format!("{}\n", json!({"id":1,"status":status})),
            )
            .unwrap();
            current.to_string_lossy().into_owned()
        } else {
            f.root.join("old").to_string_lossy().into_owned()
        };
        let o = f.run(&["select-project", p.to_str().unwrap(), &expected]);
        assert!(!o.status.success());
        assert_eq!(fs::read_dir(p).unwrap().count(), 0);
        assert!(text(&o.stderr).contains(if status.is_none() {
            "active project changed"
        } else {
            "finish recording"
        }))
    }
    let f = Fixture::new();
    let c = f.make_session("20260904-120000-current", None, true);
    fs::create_dir(c.join("notes.md.tmp")).unwrap();
    let p = f.root.join("target");
    fs::create_dir(&p).unwrap();
    assert!(
        !f.run(&["select-project", p.to_str().unwrap(), c.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(fs::read_dir(p).unwrap().count(), 0)
}

#[test]
fn create_project_validates_names_collisions_and_rolls_back() {
    let f = Fixture::new();
    let parent = f.root.join("parent");
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("keep"), "yes").unwrap();
    let o = f.ok(&[
        "create-project",
        parent.to_str().unwrap(),
        "My new project",
        "",
    ]);
    let p = parent.join("My new project");
    assert_eq!(text(&o.stdout).trim(), p.to_str().unwrap());
    assert!(p.join(".data/id").is_file());
    assert_eq!(fs::read_to_string(parent.join("keep")).unwrap(), "yes");
    for name in ["", " ", ".", "..", "../x", "a/b", "a\\b"] {
        let f = Fixture::new();
        let parent = f.root.join("parent");
        fs::create_dir(&parent).unwrap();
        let o = f.run(&["create-project", parent.to_str().unwrap(), name, ""]);
        assert!(!o.status.success());
        assert!(text(&o.stderr).contains("single nonempty folder name"));
        assert_eq!(fs::read_dir(parent).unwrap().count(), 0)
    }
    let f = Fixture::new();
    let parent = f.root.join("parent");
    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("exists")).unwrap();
    let o = f.run(&["create-project", parent.to_str().unwrap(), "exists", ""]);
    assert!(text(&o.stderr).contains("project folder already exists"));
    let f = Fixture::new();
    let missing = f.root.join("missing");
    let o = f.run(&["create-project", missing.to_str().unwrap(), "x", ""]);
    assert!(text(&o.stderr).contains("existing parent directory"));
    assert!(!missing.exists())
}

#[test]
fn start_resume_relocate_and_status_behave() {
    let f = Fixture::new();
    let o = f.ok(&["start", "remember me"]);
    let started = PathBuf::from(text(&o.stdout).trim());
    assert!(
        started
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("remember-me")
    );
    assert_eq!(registry(&f)[0], started.to_string_lossy());
    assert_eq!(
        text(&f.ok(&["status"]).stdout).trim(),
        started.to_str().unwrap()
    );
    let d = f.root.join("moved");
    fs::create_dir(&d).unwrap();
    f.ok(&["relocate", d.to_str().unwrap()]);
    assert!(d.join("notes.md").is_file());
    assert!(!started.exists());
    assert_eq!(registry(&f)[0], d.to_string_lossy());
    f.ok(&["stop"]);
    let inactive = f.run(&["status"]);
    assert_eq!(inactive.status.code(), Some(1));
    assert_eq!(text(&inactive.stdout), "inactive\n");
    let s = f.make_session("20260904-120000-resume", Some(b"draft"), false);
    let before = tree(&s);
    f.ok(&["resume", s.to_str().unwrap()]);
    assert_eq!(tree(&s), before)
}

#[test]
fn clip_captures_plain_note_renders_link_and_relocates_asset() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-clip", None, true);
    script(
        &f.bin.join("slurp"),
        "#!/bin/sh\nprintf '10,20 300x200\\n'\n",
    );
    script(
        &f.bin.join("grim"),
        "#!/bin/sh\ntest \"$1\" = -g || exit 8\ntest \"$2\" = '10,20 300x200' || exit 9\nprintf png > \"$3\"\n",
    );
    let panel = f.panel_commands(2);
    let output = f.ok(&["clip"]);
    assert_eq!(text(&output.stdout), "1\n");
    assert_eq!(panel.join().unwrap(), ["hide", "show"]);
    let entries: Vec<Value> = fs::read_to_string(s.join(".data/entries.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["ai"], false);
    assert_eq!(entries[0]["status"], "done");
    assert_eq!(entries[0]["transcript"], "");
    assert_eq!(
        entries[0]["asset_size"],
        serde_json::json!({"width":300,"height":200})
    );
    let relative = entries[0]["asset"].as_str().unwrap();
    assert!(relative.starts_with("assets/clip-"));
    assert_eq!(fs::read(s.join(relative)).unwrap(), b"png");
    assert!(
        fs::read_to_string(s.join("notes.md"))
            .unwrap()
            .contains(&format!("![Screen capture]({relative})"))
    );
    f.ok(&["render"]);
    let rendered: Value = serde_json::from_str(
        fs::read_to_string(s.join(".data/entries.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(rendered["asset_size"], entries[0]["asset_size"]);

    let destination = f.root.join("relocated");
    fs::create_dir(&destination).unwrap();
    f.ok(&["relocate", destination.to_str().unwrap()]);
    assert_eq!(fs::read(destination.join(relative)).unwrap(), b"png");
    let relocated: Value = serde_json::from_str(
        fs::read_to_string(destination.join(".data/entries.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(relocated["asset_size"], entries[0]["asset_size"]);
}

#[test]
fn clip_cancel_and_capture_failure_leave_no_note_or_asset() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-cancel", None, true);
    script(
        &f.bin.join("slurp"),
        "#!/bin/sh\nprintf 'selection cancelled\\n' >&2\nexit 1\n",
    );
    let panel = f.panel_commands(2);
    let output = f.ok(&["clip"]);
    assert!(output.stdout.is_empty());
    assert_eq!(panel.join().unwrap(), ["hide", "show"]);
    assert_eq!(
        fs::read_to_string(s.join(".data/entries.jsonl")).unwrap(),
        ""
    );
    assert!(!s.join("assets").exists());

    script(&f.bin.join("slurp"), "#!/bin/sh\nprintf '1,2 0x4\\n'\n");
    script(&f.bin.join("grim"), "#!/bin/sh\nprintf png > \"$3\"\n");
    let panel = f.panel_commands(2);
    let output = f.run(&["clip"]);
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("invalid capture dimensions"));
    assert_eq!(panel.join().unwrap(), ["hide", "show"]);
    assert_eq!(
        fs::read_to_string(s.join(".data/entries.jsonl")).unwrap(),
        ""
    );
    assert!(!s.join("assets").exists());

    script(&f.bin.join("slurp"), "#!/bin/sh\nprintf '1,2 3x4\\n'\n");
    script(&f.bin.join("grim"), "#!/bin/sh\nexit 1\n");
    let panel = f.panel_commands(2);
    let output = f.run(&["clip"]);
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("grim failed"));
    assert_eq!(panel.join().unwrap(), ["hide", "show"]);
    assert_eq!(
        fs::read_to_string(s.join(".data/entries.jsonl")).unwrap(),
        ""
    );
    assert!(!s.join("assets").exists());
}

#[test]
fn clip_rolls_back_on_session_change_and_render_failure() {
    let f = Fixture::new();
    let original = f.make_session("20260904-120000-original", None, true);
    let other = f.make_session("20260904-120001-other", None, false);
    script(
        &f.bin.join("slurp"),
        &format!(
            "#!/bin/sh\nprintf '{}\\n' > \"$XDG_RUNTIME_DIR/ui-notes/session\"\nprintf '1,2 3x4\\n'\n",
            other.display()
        ),
    );
    script(&f.bin.join("grim"), "#!/bin/sh\nprintf png > \"$3\"\n");
    let panel = f.panel_commands(2);
    let output = f.run(&["clip"]);
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("active project changed"));
    assert_eq!(panel.join().unwrap(), ["hide", "show"]);
    assert_eq!(
        fs::read_to_string(original.join(".data/entries.jsonl")).unwrap(),
        ""
    );
    assert!(!original.join("assets").exists());

    fs::write(f.pointer(), format!("{}\n", original.display())).unwrap();
    script(&f.bin.join("slurp"), "#!/bin/sh\nprintf '1,2 3x4\\n'\n");
    fs::create_dir(original.join("notes.md.tmp")).unwrap();
    let panel = f.panel_commands(2);
    let output = f.run(&["clip"]);
    assert!(!output.status.success());
    assert_eq!(panel.join().unwrap(), ["hide", "show"]);
    assert_eq!(
        fs::read_to_string(original.join(".data/entries.jsonl")).unwrap(),
        ""
    );
    assert!(original.join("assets").read_dir().unwrap().next().is_none());
}

#[test]
fn relocate_rejects_assets_collision_without_moving_content() {
    let f = Fixture::new();
    let session = f.make_session("20260904-120000-assets", None, true);
    fs::create_dir(session.join("assets")).unwrap();
    fs::write(session.join("assets/clip.png"), "source").unwrap();
    let destination = f.root.join("destination");
    fs::create_dir(&destination).unwrap();
    fs::create_dir(destination.join("assets")).unwrap();
    fs::write(destination.join("assets/keep.png"), "destination").unwrap();
    let output = f.run(&["relocate", destination.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("already contains assets"));
    assert_eq!(
        fs::read(session.join("assets/clip.png")).unwrap(),
        b"source"
    );
    assert_eq!(
        fs::read(destination.join("assets/keep.png")).unwrap(),
        b"destination"
    );
    assert!(session.join("notes.md").exists());
    assert!(!destination.join("notes.md").exists());
}

#[test]
fn resume_rejects_invalid_sessions_and_different_active() {
    for case in 0..6 {
        let f = Fixture::new();
        let s = f.make_session("20260904-120000-x", None, false);
        let target = if case == 0 {
            f.root.join("missing")
        } else {
            s.clone()
        };
        match case {
            1 => fs::remove_file(s.join(".data/entries.jsonl")).unwrap(),
            2 => fs::remove_file(s.join(".data/id")).unwrap(),
            3 => fs::write(s.join(".data/id"), "\n").unwrap(),
            4 => fs::write(s.join(".data/entries.jsonl"), "broken\n").unwrap(),
            5 => fs::write(s.join(".data/entries.jsonl"), "\"bad\"\n").unwrap(),
            _ => {}
        }
        let o = f.run(&["resume", target.to_str().unwrap()]);
        assert!(!o.status.success());
        assert!(text(&o.stderr).contains("cannot resume session"));
        assert!(!f.pointer().exists())
    }
    let f = Fixture::new();
    let c = f.make_session("20260904-120000-c", None, true);
    let o = f.make_session("20260904-120001-o", None, false);
    let r = f.run(&["resume", o.to_str().unwrap()]);
    assert!(text(&r.stderr).contains("another session is active"));
    assert_eq!(
        fs::read_to_string(f.pointer()).unwrap().trim(),
        c.to_str().unwrap()
    )
}

#[test]
fn stop_preserves_content_and_removes_only_empty_generated_sessions() {
    for draft in [Some(&b"draft"[..]), Some(&b"\n "[..]), Some(&b"\xff"[..])] {
        let f = Fixture::new();
        let s = f.make_session("20260904-120000-x", draft, true);
        f.ok(&["stop"]);
        assert!(s.exists());
        assert!(!f.pointer().exists())
    }
    for draft in [None, Some(&b""[..])] {
        let f = Fixture::new();
        let s = f.make_session("20260904-120000-x", draft, true);
        f.ok(&["stop"]);
        assert!(!s.exists())
    }
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    fs::write(s.join(".data/transcripts/001.txt"), "x").unwrap();
    f.ok(&["stop"]);
    assert!(s.exists());

    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    fs::write(s.join("notes-extra.txt"), "keep").unwrap();
    f.ok(&["stop"]);
    assert_eq!(fs::read(s.join("notes-extra.txt")).unwrap(), b"keep");

    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    fs::write(s.join(".data/context/001.json"), "keep").unwrap();
    f.ok(&["stop"]);
    assert_eq!(fs::read(s.join(".data/context/001.json")).unwrap(), b"keep");
}

#[test]
fn stop_timeout_preserves_the_exact_session_pointer_and_project() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    fs::write(s.join("notes-extra.txt"), "keep").unwrap();
    let pointer = fs::read(f.pointer()).unwrap();
    let before = tree(&s);
    let listener = UnixListener::bind(f.runtime.join("panel.sock")).unwrap();
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["cmd"], "quit");
        accepted_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        drop(stream);
    });

    let output = f.run(&["stop"]);
    accepted_rx.recv().unwrap();
    release_tx.send(()).unwrap();
    server.join().unwrap();

    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("cannot stop session:"));
    assert_eq!(fs::read(f.pointer()).unwrap(), pointer);
    assert_eq!(tree(&s), before)
}

#[test]
fn stop_flushes_panel_renders_and_honors_rejection() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    let entries = s.join(".data/entries.jsonl");
    let pointer = f.pointer();
    let h = f.panel(move |v| {
        assert_eq!(v["cmd"], "quit");
        assert!(pointer.exists());
        fs::write(
            &entries,
            format!(
                "{}\n",
                json!({"id":1,"ai":false,"status":"done","transcript":"Last edit"})
            ),
        )
        .unwrap();
        json!({"ok":true})
    });
    f.ok(&["stop"]);
    h.join().unwrap();
    assert!(
        fs::read_to_string(s.join("notes.md"))
            .unwrap()
            .contains("Last edit")
    );
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    let h = f.panel(|_| json!({"ok":false,"error":"draft could not be saved"}));
    let o = f.run(&["stop"]);
    h.join().unwrap();
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("draft could not be saved"));
    assert!(f.pointer().exists());
    assert!(s.exists())
}

#[test]
fn section_ai_help_invalid_and_render_contract() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    assert_eq!(text(&f.ok(&["section"]).stdout), "1\n");
    assert_eq!(text(&f.ok(&["section", "Checkout"]).stdout), "2\n");
    assert!(
        fs::read_to_string(s.join("notes.md"))
            .unwrap()
            .contains("## Checkout")
    );
    assert_eq!(text(&f.ok(&["ai"]).stdout), "on\n");
    assert_eq!(text(&f.ok(&["ai", "toggle"]).stdout), "off\n");
    assert_eq!(text(&f.ok(&["ai", "off"]).stdout), "off\n");
    assert!(text(&f.ok(&["help"]).stdout).contains("usage: ui-notes"));
    let bad = f.run(&["bogus"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(text(&bad.stderr).contains("run 'ui-notes help'"))
}

#[test]
fn ingest_with_fake_voxtype_state_preserves_concurrent_fields() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    fs::write(
        s.join(".data/entries.jsonl"),
        format!(
            "{}\n",
            json!({"id":1,"status":"transcribing","transcript":"panel edit","extra":"keep"})
        ),
    )
    .unwrap();
    fs::write(s.join(".data/transcripts/001.txt"), "spoken words\n").unwrap();
    fs::create_dir_all(f.root.join("runtime/voxtype")).unwrap();
    fs::write(f.root.join("runtime/voxtype/state"), "idle\n").unwrap();
    f.ok(&["ingest", "1"]);
    let v: Value = serde_json::from_str(
        fs::read_to_string(s.join(".data/entries.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(v["transcript"], "spoken words");
    assert_eq!(v["extra"], "keep");
    assert_eq!(v["status"], "done")
}

#[test]
fn ptt_ai_off_uses_file_and_creates_plain_recording_entry() {
    let f = Fixture::new();
    let s = f.make_session("20260904-120000-x", None, true);
    fs::create_dir_all(f.home.join(".config/ui-notes")).unwrap();
    fs::write(f.home.join(".config/ui-notes/ai"), "off\n").unwrap();
    script(
        &f.bin.join("voxtype"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$HOME/voxtype-args\"\nexit 0\n",
    );
    let h = f.panel(|request| {
        assert_eq!(request["cmd"], "focus");
        json!({"ok":true,"active":true,"focus":"note","id":null})
    });
    f.ok(&["ptt", "start"]);
    h.join().unwrap();
    let args = fs::read_to_string(f.home.join("voxtype-args")).unwrap();
    assert!(args.contains("record start --file="));
    assert!(args.contains(".data/transcripts/001.txt"));
    let entry: Value = serde_json::from_str(
        fs::read_to_string(s.join(".data/entries.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(entry["ai"], false);
    assert_eq!(entry["status"], "recording");
    assert!(entry.get("shot").is_none());
}

fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    fn walk(root: &Path, p: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for e in fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out)
            } else {
                out.push((
                    p.strip_prefix(root).unwrap().to_owned(),
                    fs::read(p).unwrap(),
                ))
            }
        }
    }
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

impl Fixture {
    fn backend(&self, requests: &[Value]) -> Vec<Value> {
        use std::process::Stdio;
        let mut child = Command::new(env!("CARGO_BIN_EXE_ui-notes"))
            .arg("backend")
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("HOME", &self.home)
            .env("XDG_RUNTIME_DIR", self.root.join("runtime"))
            .env("XDG_STATE_HOME", &self.state)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env_remove("UI_NOTES_SESSION_PATH")
            .env_remove("UI_NOTES_SESSION_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        for request in requests {
            writeln!(input, "{request}").unwrap();
        }
        drop(input);
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", text(&output.stderr));
        text(&output.stdout)
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

#[test]
fn backend_roundtrip_draft_note_edit_and_section_preserves_formats() {
    let f = Fixture::new();
    let session = f.make_session("backend-project", None, true);
    fs::write(
        session.join(".data/entries.jsonl"),
        "{\"id\":1,\"kind\":\"section\",\"title\":\"Initial\"}\n",
    )
    .unwrap();
    let request = |id, cmd, extra: Value| {
        let mut value = json!({"id":id,"cmd":cmd,"session":session,"token":"test-session-id"});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        value
    };
    let replies = f.backend(&[
        json!({"id":1,"cmd":"snapshot"}),
        request(2, "draft", json!({"text":"  draft\nwith newline  "})),
        request(3, "note", json!({"text":"  draft\nwith newline  "})),
        request(4, "edit", json!({"entryId":2,"text":"edited note"})),
        request(5, "edit", json!({"entryId":1,"text":"Renamed"})),
        request(6, "delete-section", json!({"entryId":2})),
        request(7, "delete-section", json!({"entryId":1})),
    ]);
    assert_eq!(replies[0]["state"]["runtimeDir"], json!(f.runtime));
    assert_eq!(replies[0]["state"]["token"], "test-session-id");
    assert_eq!(replies[1]["state"]["draft"], "  draft\nwith newline  ");
    assert_eq!(replies[2]["state"]["draft"], "");
    assert_eq!(
        replies[2]["state"]["entries"][1]["transcript"],
        "  draft\nwith newline  "
    );
    assert_eq!(replies[3]["state"]["entries"][1]["ai"], false);
    assert_eq!(replies[4]["state"]["entries"][0]["title"], "Renamed");
    assert_eq!(replies[5]["ok"], false);
    assert_eq!(replies[6]["state"]["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(session.join(".data/draft.txt")).unwrap(),
        ""
    );
    assert!(
        fs::read_to_string(session.join("notes.md"))
            .unwrap()
            .contains("edited note")
    );
    assert!(!session.join(".data/entries.jsonl.tmp").exists());
}

#[test]
fn backend_rejects_stale_path_and_token_without_touching_projects() {
    let f = Fixture::new();
    let old = f.make_session("old", Some(b"old draft"), false);
    let session = f.make_session("active", Some(b"active draft"), true);
    for (expected, token) in [(&old, "test-session-id"), (&session, "stale-token")] {
        let replies = f.backend(&["draft", "note", "edit", "delete-section", "delete-note"].map(|cmd|
            json!({"id":cmd,"cmd":cmd,"session":expected,"token":token,"text":"overwrite","entryId":1})));
        assert!(replies.iter().all(|reply| reply["ok"] == false));
    }
    assert_eq!(
        fs::read_to_string(old.join(".data/draft.txt")).unwrap(),
        "old draft"
    );
    assert_eq!(
        fs::read_to_string(session.join(".data/draft.txt")).unwrap(),
        "active draft"
    );
    assert_eq!(
        fs::read_to_string(session.join(".data/entries.jsonl")).unwrap(),
        ""
    );
}

#[test]
fn backend_delete_note_persists_and_rejects_sections_and_active_transcription() {
    let f = Fixture::new();
    let session = f.make_session("delete-note", None, true);
    fs::create_dir(session.join("assets")).unwrap();
    fs::write(session.join("assets/clip.png"), "keep asset").unwrap();
    fs::write(
        session.join(".data/entries.jsonl"),
        concat!(
            "{\"id\":1,\"kind\":\"section\",\"title\":\"Keep\"}\n",
            "{\"id\":2,\"status\":\"recording\",\"transcript\":\"Live\"}\n",
            "{\"id\":3,\"status\":\"transcribing\",\"transcript\":\"Pending\"}\n",
            "{\"id\":4,\"status\":\"done\",\"transcript\":\"Remove me\",\"asset\":\"assets/clip.png\"}\n",
            "{\"id\":5,\"status\":\"done\",\"context_status\":\"pending\",\"transcript\":\"Analyzing\"}\n",
        ),
    )
    .unwrap();
    let request = |id| json!({"id":id,"cmd":"delete-note","session":session,"token":"test-session-id","entryId":id});

    let replies = f.backend(&[
        request(1),
        request(2),
        request(3),
        request(5),
        request(4),
        request(99),
    ]);

    assert_eq!(replies[0]["ok"], false);
    assert_eq!(replies[1]["ok"], false);
    assert_eq!(replies[2]["ok"], false);
    assert_eq!(replies[3]["ok"], false);
    assert_eq!(replies[4]["ok"], true);
    assert_eq!(replies[4]["state"]["entries"].as_array().unwrap().len(), 4);
    assert_eq!(replies[5]["ok"], false);
    let stored = fs::read_to_string(session.join(".data/entries.jsonl")).unwrap();
    assert!(!stored.contains("Remove me"));
    assert!(stored.contains("Keep"));
    assert!(stored.contains("Live"));
    assert!(stored.contains("Pending"));
    assert!(stored.contains("Analyzing"));
    assert!(
        !fs::read_to_string(session.join("notes.md"))
            .unwrap()
            .contains("Remove me")
    );
    assert_eq!(
        fs::read_to_string(session.join("assets/clip.png")).unwrap(),
        "keep asset"
    );
}

#[test]
fn backend_note_rolls_back_if_draft_cannot_be_cleared() {
    let f = Fixture::new();
    let session = f.make_session("draft-failure", None, true);
    fs::create_dir(session.join(".data/draft.txt")).unwrap();
    let replies = f.backend(&[json!({"id":1,"cmd":"note","session":session,"token":"test-session-id","text":"retain on failure"})]);
    assert_eq!(replies[0]["ok"], false);
    assert_eq!(
        fs::read_to_string(session.join(".data/entries.jsonl")).unwrap(),
        ""
    );
    assert_eq!(
        fs::read_to_string(session.join("notes.md")).unwrap(),
        "Keep this Markdown exactly.\n"
    );
}

#[test]
fn backend_saved_note_remains_success_with_bad_registry_and_markdown_failure() {
    let f = Fixture::new();
    let session = f.make_session("render-failure", Some(b"draft"), true);
    fs::remove_file(session.join("notes.md")).unwrap();
    fs::create_dir(session.join("notes.md")).unwrap();
    fs::create_dir_all(f.state.join("ui-notes")).unwrap();
    fs::write(f.state.join("ui-notes/projects.json"), "invalid JSON").unwrap();
    let replies = f.backend(&[json!({"id":1,"cmd":"note","session":session,"token":"test-session-id","text":"saved once"})]);
    assert_eq!(replies[0]["ok"], true);
    assert!(
        replies[0]["warning"]
            .as_str()
            .unwrap()
            .contains("cannot render")
    );
    assert_eq!(replies[0]["state"]["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(session.join(".data/draft.txt")).unwrap(),
        ""
    );
}

#[test]
fn launcher_uses_sibling_and_rejects_unknown_plugin_response() {
    let f = Fixture::new();
    script(&f.bin.join("ui-notes-panel"), "#!/bin/sh\nexit 42\n");
    // The sibling Rust launcher must win over a legacy PATH executable.
    f.ok(&["open"]);
    script(&f.bin.join("omarchy-shell"), "#!/bin/sh\necho unknown\n");
    let output = f.run(&["open"]);
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("Install and enable"));
}

#[test]
fn backend_theme_refreshes_ghostty_only_when_a_source_changes() {
    use std::process::Stdio;
    let f = Fixture::new();
    let config = f.home.join(".config/ghostty/config");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "background = #101010\nforeground = #eeeeee\nfont-family = Test Mono\nfont-size = 8\npalette = 4=#aabbcc\n").unwrap();
    script(
        &f.bin.join("ghostty"),
        "#!/bin/sh\nprintf x >> \"$HOME/ghostty-calls\"\ncat \"$HOME/.config/ghostty/config\"\n",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_ui-notes"))
        .arg("backend")
        .env("HOME", &f.home)
        .env("XDG_CONFIG_HOME", f.home.join(".config"))
        .env("XDG_RUNTIME_DIR", f.root.join("runtime"))
        .env("XDG_STATE_HOME", &f.state)
        .env("PATH", format!("{}:/usr/bin:/bin", f.bin.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut snapshot = || {
        writeln!(input, "{}", json!({"cmd":"snapshot"})).unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["ok"], true);
        reply["state"]["theme"].clone()
    };
    let theme = snapshot();
    assert_eq!(theme["background"], "#101010");
    assert_eq!(theme["accent"], "#aabbcc");
    assert_eq!(theme["fontPointSize"], 8.0);
    assert_eq!(theme["smallPointSize"], 8.0);
    assert_eq!(snapshot(), theme);
    assert_eq!(
        fs::read_to_string(f.home.join("ghostty-calls")).unwrap(),
        "x"
    );
    fs::write(
        &config,
        "background = #fafafa\nforeground = #111111\nfont-family = Other Mono\nfont-size = 9\n",
    )
    .unwrap();
    let updated = snapshot();
    assert_eq!(updated["background"], "#fafafa");
    assert_eq!(updated["fontFamily"], "Other Mono");
    assert_eq!(updated["fontPointSize"], 9.0);
    assert_eq!(
        fs::read_to_string(f.home.join("ghostty-calls")).unwrap(),
        "xx"
    );
    drop(input);
    assert!(child.wait().unwrap().success());
}

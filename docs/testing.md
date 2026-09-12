# Testing Omatate

Run the service tests with Node.js:

```sh
node --test --test-isolation=none tests/service.test.mjs
```

These exercise the same JavaScript service imported by QML. Tests use temporary
projects and cover concurrent saves, project identity, draft recovery, failure
handling, captures, dictation, relocation, Markdown, shortcuts, and colors.
Node.js is a development tool, not a plugin dependency.

Run the Quickshell service integration check:

```sh
python3 tests/check-service.py
```

It starts an isolated Quickshell process on the offscreen platform, with a
private home and runtime directory. It checks actual FileView and Process
behavior, empty writes, note saves, project switching, command failures, and
draft preservation. It does not access your projects or microphone.

Run the Qt Quick component tests:

```sh
./tests/run-qml-tests.sh
```

The runner finds `qmltestrunner` on PATH or in `/usr/lib/qt6/bin`, selects the
offscreen platform and software renderer, and changes no desktop settings.
Extra runner arguments pass through, for example `-v2` or `-functions`.

Run the panel and command launcher integration check on a Wayland desktop:

```sh
python3 tests/check-panel.py
```

It briefly opens the real panel with temporary projects and checks the project
chooser, note submission, CLI arguments, project switching, and session closure.
It uses an isolated shell instance and does not change your installed plugin.

Measure what the plugin costs the shell at startup on a Wayland desktop:

```sh
python3 tests/bench-startup.py
```

It loads `Plugin.qml` into a disposable shell with a seeded project registry
and reports synchronous instantiation time, time to a ready service, processes
spawned, and resident memory added. The panel UI is not opened, matching what
the Omarchy shell pays for a `keepLoaded` plugin at login.

For the folder dialog check on a running Wayland desktop:

```sh
python3 tests/check-folder-picker.py
```

This opens a disposable panel for a few seconds without loading your projects.
A final desktop check should cover focus across windows, captures at your
monitor scale, microphone behavior, and analysis using your configured tools.

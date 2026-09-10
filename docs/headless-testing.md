# Headless QML tests

Run the component tests from the checkout root:

```sh
./tests/run-qml-tests.sh
```

The runner selects the offscreen platform and software rendering for its own
process, clears the inherited RHI backend and platform theme, and uses the Basic
Qt Quick Controls style. No desktop settings change. It finds `qmltestrunner` on
PATH or at `/usr/lib/qt6/bin/qmltestrunner`.

Extra arguments pass through to the runner:

```sh
./tests/run-qml-tests.sh -v2
./tests/run-qml-tests.sh -functions
```

The tests cover component construction, theme bindings, sizing, text entry,
button clicks, and the note-deletion shortcut. They do not start Quickshell or
read user projects. Live rendering, focus across windows, microphone behavior,
and shell integration still need a desktop check.

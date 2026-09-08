import QtQuick
import QtQuick.Controls
import QtTest
import "../../qml" as Notes

Rectangle {
    id: window
    width: 480
    height: 320
    visible: true

    QtObject {
        id: testTheme
        property color accent: "#7aa2f7"
        property color control: "#1f2335"
        property color cursor: "#c0caf5"
        property color foreground: "#c0caf5"
        property color hair: "#414868"
        property color muted: "#9aa5ce"
        property color selectionBg: "#364a82"
        property color selectionFg: "#ffffff"
        property string fontFamily: "sans-serif"
        property real fontPointSize: 11
        property real smallPointSize: 9
    }

    Notes.NoteButton {
        id: button
        theme: testTheme
        text: "Save"
        x: 20
        y: 20
    }

    Notes.NoteField {
        id: field
        theme: testTheme
        x: 20
        y: 70
        width: 220
    }

    Notes.NoteEditor {
        id: editor
        theme: testTheme
        x: 20
        y: 120
        width: 220
        height: 120
    }

    SignalSpy {
        id: clickSpy
        target: button
        signalName: "clicked"
    }

    SignalSpy {
        id: controlDeleteSpy
        target: editor
        signalName: "controlDeletePressed"
    }

    TestCase {
        name: "Controls"
        when: windowShown

        function init() {
            clickSpy.clear()
            controlDeleteSpy.clear()
            field.clear()
            editor.clear()
            editor.captureControlDelete = false
        }

        function test_buttonClickEmitsClicked() {
            mouseClick(button)
            compare(clickSpy.count, 1)
        }

        function test_fieldAcceptsTypedText() {
            field.forceActiveFocus()
            verify(field.activeFocus)
            keyClick(Qt.Key_H)
            keyClick(Qt.Key_I)
            compare(field.text, "hi")
        }

        function test_editorPreservesPlainTextAndWraps() {
            const literalMarkup = "<b>plain</b> text that should wrap across several visual lines"
            editor.width = 100
            editor.text = literalMarkup
            compare(editor.textFormat, TextEdit.PlainText)
            compare(editor.text, literalMarkup)
            compare(editor.wrapMode, TextEdit.Wrap)
            verify(editor.lineCount > 1)
        }

        function test_editorCapturesControlDeleteForNoteRemoval() {
            editor.text = "saved note"
            editor.captureControlDelete = true
            editor.forceActiveFocus()
            verify(editor.activeFocus)

            keyClick(Qt.Key_Delete, Qt.ControlModifier)

            compare(controlDeleteSpy.count, 1)
            compare(editor.text, "saved note")
        }
    }
}

import QtQuick
import QtQuick.Controls as QQC

QQC.TextArea {
    id: editor
    required property var theme
    property bool section: false
    property bool bordered: false
    property bool captureControlDelete: false
    signal controlDeletePressed()
    textFormat: TextEdit.PlainText
    wrapMode: section ? TextEdit.NoWrap : TextEdit.Wrap
    verticalAlignment: section ? TextEdit.AlignVCenter : TextEdit.AlignTop
    selectByMouse: true
    activeFocusOnTab: true
    color: theme.foreground
    selectionColor: theme.selectionBg
    selectedTextColor: theme.selectionFg
    placeholderTextColor: theme.muted
    font.family: theme.fontFamily
    font.pointSize: section ? theme.smallPointSize : theme.fontPointSize
    padding: section ? 3 : 5
    leftPadding: section ? 3 : 5
    rightPadding: section ? 3 : 5
    topPadding: section ? 1 : 5
    bottomPadding: section ? 1 : 5
    implicitHeight: section ? Math.max(18, contentHeight + 2) : contentHeight + topPadding + bottomPadding
    Keys.onShortcutOverride: event => {
        if (captureControlDelete && event.key === Qt.Key_Delete && event.modifiers === Qt.ControlModifier)
            event.accepted = true
    }
    Keys.onPressed: event => {
        if (captureControlDelete && event.key === Qt.Key_Delete && event.modifiers === Qt.ControlModifier) {
            event.accepted = true
            if (!event.isAutoRepeat) controlDeletePressed()
        }
    }
    background: Rectangle {
        color: editor.theme.control
        border.width: 1
        border.color: editor.activeFocus ? editor.theme.accent : (editor.bordered ? editor.theme.hair : "transparent")
    }
    cursorDelegate: Rectangle {
        color: editor.theme.cursor
        width: 1
        height: editor.cursorRectangle.height
        visible: editor.activeFocus && editor.cursorVisible
    }
}

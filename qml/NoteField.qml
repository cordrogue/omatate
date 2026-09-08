import QtQuick
import QtQuick.Controls as QQC

QQC.TextField {
    id: field
    required property var theme
    selectByMouse: true
    activeFocusOnTab: true
    color: theme.foreground
    selectionColor: theme.selectionBg
    selectedTextColor: theme.selectionFg
    placeholderTextColor: theme.muted
    font.family: theme.fontFamily
    font.pointSize: theme.fontPointSize
    leftPadding: 6
    rightPadding: 6
    topPadding: 1
    bottomPadding: 1
    implicitHeight: Math.max(24, contentHeight + 2)
    background: Rectangle {
        color: field.theme.control
        border.width: 1
        border.color: field.activeFocus ? field.theme.accent : field.theme.hair
    }
    cursorDelegate: Rectangle {
        color: field.theme.cursor
        width: 1
        height: field.cursorRectangle.height
        visible: field.activeFocus && field.cursorVisible
    }
}

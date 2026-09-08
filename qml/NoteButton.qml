import QtQuick
import QtQuick.Controls as QQC

QQC.Button {
    id: button
    required property var theme
    property bool selected: false
    property bool bare: false
    property string iconName: ""
    property string tooltipText: ""
    property color textColor: selected ? theme.accent : (hovered ? theme.foreground : theme.muted)
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true
    implicitWidth: iconName ? 32 : Math.max(32, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: iconName ? 24 : Math.max(24, implicitContentHeight + topPadding + bottomPadding)
    leftPadding: 5
    rightPadding: 5
    topPadding: 1
    bottomPadding: 1
    font.family: theme.fontFamily
    font.pointSize: theme.fontPointSize
    palette.buttonText: textColor
    contentItem: Item {
        implicitWidth: button.iconName ? 14 : label.implicitWidth
        implicitHeight: button.iconName ? 14 : label.implicitHeight
        opacity: button.enabled ? 1 : 0.45
        NoteIcon {
            anchors.centerIn: parent
            width: 14
            height: 14
            visible: !!button.iconName
            name: button.iconName
            color: button.textColor
        }
        Text {
            id: label
            anchors.fill: parent
            visible: !button.iconName
            text: button.text
            font: button.font
            color: button.textColor
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
    background: Rectangle {
        color: button.theme.control
        border.width: 1
        border.color: button.activeFocus ? button.theme.accent : (button.bare && !button.hovered ? "transparent" : button.theme.hair)
    }
    QQC.ToolTip.visible: hovered && tooltipText.length > 0
    QQC.ToolTip.text: tooltipText
    QQC.ToolTip.delay: 600
    Accessible.name: tooltipText || text
}

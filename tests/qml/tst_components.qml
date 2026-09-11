import QtQuick
import QtTest
import "../../qml" as Omatate

TestCase {
    name: "OmatateComponents"

    QtObject {
        id: testTheme
        readonly property color accent: "#7aa2f7"
        readonly property color foreground: "#c0caf5"
        readonly property color muted: "#565f89"
        readonly property color control: "#1f2335"
        readonly property color hair: "#3b4261"
        readonly property color selectionBg: "#33467c"
        readonly property color selectionFg: "#ffffff"
        readonly property color cursor: "#bb9af7"
        readonly property string fontFamily: "Sans Serif"
        readonly property real fontPointSize: 11
        readonly property real smallPointSize: 9
    }

    Component {
        id: buttonComponent
        Omatate.NoteButton {}
    }

    Component {
        id: fieldComponent
        Omatate.NoteField {}
    }

    Component {
        id: editorComponent
        Omatate.NoteEditor {}
    }

    Component {
        id: iconComponent
        Omatate.NoteIcon {}
    }

    function test_button_state_and_sizing() {
        const button = createTemporaryObject(buttonComponent, this, {
            theme: testTheme,
            text: "Save"
        })
        verify(button)
        compare(button.textColor, testTheme.muted)
        compare(button.focusPolicy, Qt.StrongFocus)
        verify(button.implicitWidth >= 32)
        verify(button.implicitHeight >= 24)

        button.selected = true
        compare(button.textColor, testTheme.accent)

        button.iconName = "plus"
        compare(button.implicitWidth, 32)
        compare(button.implicitHeight, 24)
    }

    function test_icons_use_theme_accent_and_selected_border() {
        const button = createTemporaryObject(buttonComponent, this, {
            theme: testTheme,
            iconName: "sparkle"
        })
        verify(button)
        compare(button.contentItem.children[0].color, testTheme.accent)
        button.selected = true
        compare(button.contentItem.children[0].color, testTheme.accent)
        compare(button.background.border.color, testTheme.accent)
        button.theme = {accent: "#faa968", foreground: "#f6dcac", muted: "#2a6b78",
            control: "#05182e", hair: "#40f6dcac", fontFamily: "Sans Serif", fontPointSize: 11}
        compare(button.contentItem.children[0].color, "#faa968")
    }

    function test_field_applies_theme_and_geometry() {
        const field = createTemporaryObject(fieldComponent, this, {
            theme: testTheme,
            text: "query"
        })
        verify(field)
        compare(field.color, testTheme.foreground)
        compare(field.selectionColor, testTheme.selectionBg)
        compare(field.selectedTextColor, testTheme.selectionFg)
        compare(field.placeholderTextColor, testTheme.muted)
        compare(field.background.color, testTheme.control)
        compare(field.background.border.color, testTheme.hair)
        compare(field.leftPadding, 6)
        compare(field.rightPadding, 6)
        verify(field.implicitHeight >= 24)
    }

    function test_editor_section_mode() {
        const editor = createTemporaryObject(editorComponent, this, {
            theme: testTheme,
            text: "A section"
        })
        verify(editor)
        compare(editor.textFormat, TextEdit.PlainText)
        compare(editor.wrapMode, TextEdit.Wrap)
        compare(editor.verticalAlignment, TextEdit.AlignTop)
        compare(editor.padding, 5)
        compare(editor.background.border.color, "#00000000")

        editor.bordered = true
        compare(editor.background.border.color, testTheme.hair)

        editor.section = true
        compare(editor.wrapMode, TextEdit.NoWrap)
        compare(editor.verticalAlignment, TextEdit.AlignVCenter)
        compare(editor.padding, 3)
        compare(editor.topPadding, 1)
        verify(editor.implicitHeight >= 18)
    }

    function test_icon_catalog_and_defaults() {
        const icon = createTemporaryObject(iconComponent, this, {
            name: "search",
            color: testTheme.accent
        })
        verify(icon)
        compare(icon.implicitWidth, 14)
        compare(icon.implicitHeight, 14)
        compare(icon.color, testTheme.accent)
        verify(icon.paths.search.length > 0)
        verify(icon.paths.plus.length > 0)
        compare(icon.paths.unknown, undefined)
    }
}

import QtQuick
import QtQuick.Shapes

Item {
    id: icon
    property string name: ""
    property color color: "white"
    implicitWidth: 14
    implicitHeight: 14

    // Lucide geometry, rendered from its original 24-unit viewBox.
    // Original SVGs, attribution, and ISC/Feather MIT notices are in icons/.
    readonly property var paths: ({
        "search": "m21 21-4.34-4.34 M19 11 A8 8 0 1 1 3 11 A8 8 0 1 1 19 11 Z",
        "folder": "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
        "capture": "M3 7V5a2 2 0 0 1 2-2h2 M17 3h2a2 2 0 0 1 2 2v2 M21 17v2a2 2 0 0 1-2 2h-2 M7 21H5a2 2 0 0 1-2-2v-2",
        "sparkle": "M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z M20 2v4 M22 4h-4 M6 20 A2 2 0 1 1 2 20 A2 2 0 1 1 6 20 Z",
        "plus": "M5 12h14 M12 5v14",
        "collapse": "m18 15-6-6-6 6",
        "expand": "m6 9 6 6 6-6",
        "close": "M18 6 6 18 M6 6 18 18"
    })

    Shape {
        anchors.centerIn: parent
        width: 24
        height: 24
        scale: 14 / 24
        ShapePath {
            strokeColor: icon.color
            strokeWidth: 1.5 * 24 / 14
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            fillColor: "transparent"
            PathSvg { path: icon.paths[icon.name] || "" }
        }
    }
}

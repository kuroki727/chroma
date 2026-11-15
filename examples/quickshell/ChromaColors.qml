// Place this component beside shell.qml after adapting palettePath.
import QtQuick
import Quickshell.Io

QtObject {
    id: root
    required property string palettePath

    readonly property var colors: {
        try {
            return JSON.parse(file.text());
        } catch (error) {
            return {background: "#17130F", foreground: "#E8DCC8", primary: "#B98252"};
        }
    }

    property FileView file: FileView {
        path: root.palettePath
        blockLoading: true
        watchChanges: true
        onFileChanged: this.reload()
    }
}

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// Make cxx-qt-build's qmlcachegen step produce *bytecode only*, so the binary
/// does not import Qt private API.
///
/// WHY: cxx-qt-build runs `qmlcachegen` unconditionally, with hard-coded
/// arguments and a cleared environment, which makes it emit AOT-compiled C++
/// for every QML file. That C++ calls `QQmlPrivate::AOTCompiledContext::mark`
/// (and friends), which libQt6Qml exports under the `Qt_6_PRIVATE_API` symbol
/// version. A binary that imports it only loads against the exact Qt build it
/// was compiled with, so every distro package would need a patch-exact Qt pin
/// (and on a rolling distro every Qt update would uninstall the app). With
/// `--only-bytecode` the same QML is still precompiled, just to bytecode that
/// the QML engine interprets, using public API only.
///
/// HOW: there is no cxx-qt-build option for this. The one hook is that it finds
/// the tool through `$QMAKE` -> `qmake -query QT_HOST_LIBEXECS`. We point QMAKE
/// at a small shim (generated in OUT_DIR) that answers that query with a
/// directory of symlinks to the real Qt tools, except `qmlcachegen`, which is a
/// wrapper adding `--only-bytecode`. Everything else is forwarded untouched.
///
/// Any failure falls back to the stock (AOT) behaviour with a cargo warning.
/// `OPENGG_QML_AOT=1` forces the stock behaviour.
#[cfg(unix)]
fn install_bytecode_only_qmlcachegen() {
    println!("cargo:rerun-if-env-changed=QMAKE");
    println!("cargo:rerun-if-env-changed=OPENGG_QML_AOT");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("OPENGG_QML_AOT").is_ok_and(|v| !v.is_empty() && v != "0") {
        return;
    }
    if let Err(e) = try_install_shim() {
        println!(
            "cargo:warning=qmlcachegen bytecode-only shim disabled ({e}); the binary will import \
             Qt private API and only run against the Qt it was built with"
        );
    }
}

#[cfg(unix)]
fn try_install_shim() -> Result<(), String> {
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn query(qmake: &Path, var: &str) -> Option<String> {
        let out = Command::new(qmake).args(["-query", var]).output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (out.status.success() && !s.is_empty() && !s.starts_with('*')).then_some(s)
    }
    fn is_qt6_qmake(p: &Path) -> bool {
        query(p, "QT_VERSION").is_some_and(|v| v.starts_with("6."))
    }
    // POSIX single-quote a path for embedding in a /bin/sh script.
    fn sq(p: &Path) -> Result<String, String> {
        let s = p.to_str().ok_or("non-UTF-8 path")?;
        Ok(format!("'{}'", s.replace('\'', "'\\''")))
    }

    // The real qmake: $QMAKE if set, else the first Qt6 one on PATH (mirrors
    // what cxx-qt-build would have picked).
    let real_qmake: PathBuf = match std::env::var_os("QMAKE") {
        Some(q) if !q.is_empty() => PathBuf::from(q),
        _ => ["qmake6", "qmake-qt6", "qmake"]
            .iter()
            .find_map(|n| {
                std::env::split_paths(&std::env::var_os("PATH")?)
                    .map(|d| d.join(n))
                    .find(|p| p.is_file() && is_qt6_qmake(p))
            })
            .ok_or("no Qt6 qmake found")?,
    };
    let real_qmake = std::fs::canonicalize(&real_qmake).unwrap_or(real_qmake);
    if !is_qt6_qmake(&real_qmake) {
        return Err(format!("{} is not a working Qt6 qmake", real_qmake.display()));
    }

    // Merge the real libexec + bin dirs (cxx-qt-build looks in libexecs first,
    // then bins) into one directory of symlinks.
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR unset")?);
    let shim = out_dir.join("qml-bytecode-shim");
    let _ = std::fs::remove_dir_all(&shim);
    let tools = shim.join("tools");
    std::fs::create_dir_all(&tools).map_err(|e| e.to_string())?;
    let mut real_qmlcachegen = None;
    for var in [
        "QT_HOST_LIBEXECS/get",
        "QT_HOST_LIBEXECS",
        "QT_INSTALL_LIBEXECS",
        "QT_HOST_BINS/get",
        "QT_HOST_BINS",
        "QT_INSTALL_BINS",
    ] {
        let Some(dir) = query(&real_qmake, var) else { continue };
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for ent in rd.flatten() {
            let name = ent.file_name();
            let link = tools.join(&name);
            if name == "qmlcachegen" {
                real_qmlcachegen.get_or_insert(ent.path());
            } else if !link.exists() && link.symlink_metadata().is_err() {
                let _ = symlink(ent.path(), &link);
            }
        }
    }
    let real_qmlcachegen = real_qmlcachegen.ok_or("qmlcachegen not found")?;

    // Probe: an old qmlcachegen without the flag must not be broken by the shim.
    let help = Command::new(&real_qmlcachegen)
        .arg("--help")
        .output()
        .map_err(|e| e.to_string())?;
    let help = String::from_utf8_lossy(&help.stdout).into_owned()
        + &String::from_utf8_lossy(&help.stderr);
    if !help.contains("--only-bytecode") {
        return Err("this qmlcachegen has no --only-bytecode".into());
    }

    // qmlcachegen wrapper. cxx-qt-build clears the environment, so set PATH.
    // In bytecode-only mode the generated C++ still contains a call to
    // AOTLookupValidation::validateLookupSignature (Qt 6.12), which is private
    // API and has no body to validate; replace it with `true`.
    let wrapper = format!(
        r#"#!/bin/sh
PATH=/usr/bin:/bin:/usr/local/bin
real={real}
case "$*" in *--help*|*-help*|*--version*) exec "$real" "$@" ;; esac
"$real" "$@" --only-bytecode || exit $?
out=
prev=
for a in "$@"; do
    if [ "$prev" = "-o" ]; then out=$a; fi
    prev=$a
done
if [ -n "$out" ] && [ -f "$out" ]; then
    sed -i 's/QQmlPrivate::AOTLookupValidation::validateLookupSignature([^()]*)/true/g' "$out" || exit $?
fi
"#,
        real = sq(&real_qmlcachegen)?
    );
    let qcg = tools.join("qmlcachegen");
    std::fs::write(&qcg, wrapper).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&qcg, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;

    // qmake shim.
    let qmake_shim = shim.join("qmake");
    let script = format!(
        r#"#!/bin/sh
case "$*" in
    "-query QT_HOST_LIBEXECS"|"-query QT_HOST_LIBEXECS/get") echo {tools}; exit 0 ;;
esac
exec {real} "$@"
"#,
        tools = sq(&tools)?,
        real = sq(&real_qmake)?
    );
    std::fs::write(&qmake_shim, script).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&qmake_shim, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    if !is_qt6_qmake(&qmake_shim) {
        return Err("generated qmake shim does not work".into());
    }

    // Must be in this process's environment: cxx-qt-build reads $QMAKE itself.
    // SAFETY-free on edition 2021; build scripts are single-threaded here.
    std::env::set_var("QMAKE", &qmake_shim);
    Ok(())
}

#[cfg(not(unix))]
fn install_bytecode_only_qmlcachegen() {}

fn main() {
    install_bytecode_only_qmlcachegen();

    // Theme.qml uses `pragma Singleton`; it MUST be registered with `.singleton(true)`
    // so the generated qmldir emits `singleton Theme ...`. Without this the `Theme`
    // reference (e.g. `Theme.bg`) resolves to undefined at runtime and colors fall
    // back to Qt defaults (white bg / black text).
    let qml_files: Vec<QmlFile> = vec![
        QmlFile::from("qml/Main.qml"),
        QmlFile::from("qml/Titlebar.qml"),
        QmlFile::from("qml/Sidebar.qml"),
        QmlFile::from("qml/Theme.qml").singleton(true),
        QmlFile::from("qml/Icons.qml").singleton(true),
        QmlFile::from("qml/TourController.qml").singleton(true),
        QmlFile::from("qml/TourOverlay.qml"),
        QmlFile::from("qml/pages/HomePage.qml"),
        QmlFile::from("qml/pages/MixerPage.qml"),
        QmlFile::from("qml/pages/ClipsPage.qml"),
        QmlFile::from("qml/pages/ClipEditorPage.qml"),
        QmlFile::from("qml/pages/DevicesPage.qml"),
        QmlFile::from("qml/pages/SettingsPage.qml"),
        QmlFile::from("qml/pages/settings/GeneralPanel.qml"),
        QmlFile::from("qml/pages/settings/LanguagePanel.qml"),
        QmlFile::from("qml/pages/settings/NotificationsPanel.qml"),
        QmlFile::from("qml/pages/settings/ShortcutsPanel.qml"),
        QmlFile::from("qml/pages/settings/StoragePanel.qml"),
        QmlFile::from("qml/pages/settings/TrackManagementPanel.qml"),
        QmlFile::from("qml/pages/settings/AboutPanel.qml"),
        QmlFile::from("qml/pages/settings/CaptureSoundPanel.qml"),
        QmlFile::from("qml/pages/settings/MixerRoutingPanel.qml"),
        QmlFile::from("qml/pages/settings/ExtensionsPanel.qml"),
        QmlFile::from("qml/pages/settings/StorePanel.qml"),
        QmlFile::from("qml/pages/settings/ComingSoonPanel.qml"),
        QmlFile::from("qml/components/VideoPlayer.qml"),
        QmlFile::from("qml/components/ClipVideoSurface.qml"),
        QmlFile::from("qml/components/ClipGlVideoItem.qml"),
        QmlFile::from("qml/components/ChannelStrip.qml"),
        QmlFile::from("qml/components/DeviceCard.qml"),
            QmlFile::from("qml/components/ConnectionBadge.qml"),
            QmlFile::from("qml/components/PhotoDropZone.qml"),
            QmlFile::from("qml/components/DeviceDetailPanel.qml"),
        QmlFile::from("qml/components/HotspotMarker.qml"),
        QmlFile::from("qml/components/ButtonMapEditor.qml"),
        QmlFile::from("qml/components/AppBox.qml"),
        QmlFile::from("qml/components/ClipCard.qml"),
        QmlFile::from("qml/components/ClipContextMenu.qml"),
        QmlFile::from("qml/components/ClipsBarButton.qml"),
        QmlFile::from("qml/components/ClipListRow.qml"),
        QmlFile::from("qml/components/IconToggle.qml"),
        QmlFile::from("qml/components/SegmentedToggle.qml"),
        QmlFile::from("qml/components/SettingsCard.qml"),
        QmlFile::from("qml/components/PlayerButton.qml"),
        QmlFile::from("qml/components/RecordingControl.qml"),
        QmlFile::from("qml/components/EditorTrackLane.qml"),
        QmlFile::from("qml/components/EditorButton.qml"),
        QmlFile::from("qml/components/EditorField.qml"),
        QmlFile::from("qml/components/ExportDialog.qml"),
        QmlFile::from("qml/components/ExportFieldRow.qml"),
        QmlFile::from("qml/components/ChoiceChip.qml"),
        QmlFile::from("qml/components/TrimHandle.qml"),
        QmlFile::from("qml/components/Icon.qml"),
        QmlFile::from("qml/components/SettingsHeading.qml"),
        QmlFile::from("qml/components/IconGallery.qml"),
        QmlFile::from("qml/components/InfoIcon.qml"),
        QmlFile::from("qml/components/ToggleSwitch.qml"),
        QmlFile::from("qml/components/RecorderInstallHelper.qml"),
        QmlFile::from("qml/components/HSlider.qml"),
        QmlFile::from("qml/components/SliderHandle.qml"),
        QmlFile::from("qml/components/VSlider.qml"),
        QmlFile::from("qml/components/FlyoutPopup.qml"),
        QmlFile::from("qml/components/Tip.qml"),
        QmlFile::from("qml/components/CommandLine.qml"),
        QmlFile::from("qml/components/ErrorText.qml"),
        QmlFile::from("qml/components/TrackColorSwatch.qml"),
        QmlFile::from("qml/components/SelectField.qml"),
        QmlFile::from("qml/components/WheelScroller.qml"),
        QmlFile::from("qml/components/GraphicEQ.qml"),
        QmlFile::from("qml/components/DspControls.qml"),
        QmlFile::from("qml/components/GlobalHotkeys.qml"),
    ];

    CxxQtBuilder::new_qml_module(QmlModule::new("com.opengg.app").qml_files(qml_files))
        // Rust cxx-qt QObjects (registered into the com.opengg.app module).
        .files([
            "src/i18n.rs",
            "src/audio.rs",
            "src/audio_mixer.rs",
            "src/device.rs",
            "src/clips.rs",
            "src/editor.rs",
            "src/extensions.rs",
            "src/hotkeys.rs",
            "src/eq.rs",
            "src/recording.rs",
            "src/screenshot.rs",
            "src/settings.rs",
            "src/storage.rs",
            "src/system.rs",
            "src/theme.rs",
        ])
        // Hand-written C++ for the three Qt facts the GStreamer player needs
        // that cxx-qt cannot express — see src/cpp/video_bridge.cpp.
        .cpp_file("src/cpp/video_bridge.cpp")
        // Qt Multimedia (MediaPlayer/VideoOutput) for the clip player — the
        // GStreamer backend is the PoC-validated Wayland-native path (plan §7).
        .qt_module("Multimedia")
        // QQuickWindow/QQuickItem for video_bridge.cpp.
        .qt_module("Quick")
        .build();
}

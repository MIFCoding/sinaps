use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=../android/LocalNetworkPlugin.kt");
    println!("cargo:rerun-if-changed=icons/android");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        prepare_android().expect("Не удалось подготовить разрешение локальной сети Android");
    }
    tauri_build::build()
}

fn prepare_android() -> std::io::Result<()> {
    // Исходник хранится вне gen, чтобы запрос разрешения не исчезал после android init.
    let main = Path::new("gen/android/app/src/main");
    copy_directory(Path::new("icons/android"), &main.join("res"))?;
    let kotlin_dir = main.join("java/com/synapse/compass");
    fs::create_dir_all(&kotlin_dir)?;
    let source = fs::read("../android/LocalNetworkPlugin.kt")?;
    let destination = kotlin_dir.join("LocalNetworkPlugin.kt");
    if fs::read(&destination).ok().as_deref() != Some(source.as_slice()) {
        fs::write(destination, source)?;
    }
    let manifest_path = main.join("AndroidManifest.xml");
    let manifest = fs::read_to_string(&manifest_path)?;
    if !manifest.contains("android.permission.ACCESS_LOCAL_NETWORK") {
        let marker = "<uses-permission android:name=\"android.permission.INTERNET\" />";
        if !manifest.contains(marker) {
            return Err(std::io::Error::other(
                "В манифесте не найдено разрешение INTERNET",
            ));
        }
        fs::write(manifest_path, manifest.replacen(marker, &format!("{marker}\n    <uses-permission android:name=\"android.permission.ACCESS_LOCAL_NETWORK\" />"), 1))?;
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for item in fs::read_dir(source)? {
        let item = item?;
        let target = destination.join(item.file_name());
        if item.file_type()?.is_dir() {
            copy_directory(&item.path(), &target)?;
        } else {
            let bytes = fs::read(item.path())?;
            if fs::read(&target).ok().as_deref() != Some(bytes.as_slice()) {
                fs::write(target, bytes)?;
            }
        }
    }
    Ok(())
}

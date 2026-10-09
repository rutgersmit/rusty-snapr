//! Embeds the icon, manifest and version info into Snapr.exe.
//!
//! The version comes from `SNAPR_VERSION` (set by the release workflow from the tag, e.g. "v0.2.0"
//! or "0.2-beta") and falls back to the version in Cargo.toml.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-env-changed=SNAPR_VERSION");

    let version = env::var("SNAPR_VERSION")
        .ok()
        .map(|v| v.trim().trim_start_matches(['v', 'V']).to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=SNAPR_VERSION={version}");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let [major, minor, patch] = numeric_parts(&version);
    let assets = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets");
    // Resource compilers accept forward slashes, which avoids escaping backslashes.
    let asset = |name: &str| assets.join(name).to_string_lossy().replace('\\', "/");

    let rc = format!(
        r#"1 ICON "{icon}"
1 24 "{manifest}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Snapr"
      VALUE "FileDescription", "Snapr (Rust)"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "Snapr"
      VALUE "OriginalFilename", "Snapr.exe"
      VALUE "ProductName", "Snapr (Rust)"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = asset("snapr.ico"),
        manifest = asset("app.manifest"),
    );

    let rc_path = Path::new(&env::var("OUT_DIR").unwrap()).join("snapr.rc");
    fs::write(&rc_path, rc).unwrap();
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}

/// "0.2" -> [0, 2, 0]; "1.2.3-beta" -> [1, 2, 3]. Anything unparsable becomes 0.
fn numeric_parts(version: &str) -> [u16; 3] {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let mut parts = [0u16; 3];
    for (slot, part) in parts.iter_mut().zip(core.split('.')) {
        *slot = part.parse().unwrap_or(0);
    }
    parts
}

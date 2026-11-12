fn main() {
    println!("cargo:rerun-if-changed=src/win_vcam/opencam_vcam.cpp");
    println!("cargo:rerun-if-changed=src/win_vcam/opencam_vcam.def");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        build_windows_vcam_dll();
    }
}

#[cfg(target_os = "windows")]
fn build_windows_vcam_dll() {
    use std::path::PathBuf;

    let target = std::env::var("TARGET").expect("TARGET env var missing");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR env var missing"));
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    let cpp_path = manifest_dir.join("src/win_vcam/opencam_vcam.cpp");
    let def_path = manifest_dir.join("src/win_vcam/opencam_vcam.def");
    let dll_out = out_dir.join("opencam_vcam.dll");
    let obj_out = out_dir.join("opencam_vcam.obj");

    let tool = cc::windows_registry::find_tool(&target, "cl.exe")
        .expect("Failed to find MSVC cl.exe to build opencam_vcam.dll");

    let mut cmd = tool.to_command();
    cmd.arg("/nologo")
        .arg("/O2")
        .arg("/MT")
        .arg("/EHsc")
        .arg("/LD")
        .arg(format!("/Fo{}", obj_out.display()))
        .arg(format!("/Fe{}", dll_out.display()))
        .arg(&cpp_path)
        .arg("/link")
        .arg(format!("/DEF:{}", def_path.display()))
        .arg("ole32.lib")
        .arg("oleaut32.lib")
        .arg("strmiids.lib")
        .arg("uuid.lib")
        .arg("advapi32.lib");

    let status = cmd.status().expect("Failed to execute cl.exe");
    if !status.success() {
        panic!("cl.exe failed to compile opencam_vcam.dll with status: {}", status);
    }
}

#[cfg(not(target_os = "windows"))]
fn build_windows_vcam_dll() {}

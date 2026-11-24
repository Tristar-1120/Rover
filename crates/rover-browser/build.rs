use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/windows/rover.rc");
    println!("cargo:rerun-if-changed=../../packaging/windows/rover.ico");
    println!("cargo:rerun-if-changed=../../assets/rover-icon.png");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
        let rc_dir = manifest_dir.join("../../packaging/windows");
        let rc_file = rc_dir.join("rover.rc");

        if target_env == "msvc" {
            let res_file = out_dir.join("rover_res.res");
            let status = Command::new("llvm-rc")
                .arg("/I")
                .arg(&rc_dir)
                .arg("/FO")
                .arg(&res_file)
                .arg(&rc_file)
                .status()
                .expect("Failed to execute llvm-rc");
            assert!(status.success(), "llvm-rc exited with error");
            println!("cargo:rustc-link-arg={}", res_file.display());
        } else {
            let res_obj = out_dir.join("rover_res.o");
            let status = Command::new("x86_64-w64-mingw32-windres")
                .arg("-I")
                .arg(&rc_dir)
                .arg("-i")
                .arg(&rc_file)
                .arg("-O")
                .arg("coff")
                .arg("-o")
                .arg(&res_obj)
                .status()
                .expect("Failed to execute x86_64-w64-mingw32-windres");
            assert!(
                status.success(),
                "x86_64-w64-mingw32-windres exited with error"
            );
            println!("cargo:rustc-link-arg={}", res_obj.display());
        }
    }
}

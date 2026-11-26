use std::{env, path::PathBuf, process::Command};

fn run(stage: &str, command: &mut Command) -> Vec<u8> {
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("test engine: {stage} could not start ({:?})", e.kind()));
    if !output.status.success() {
        if env::var_os("MDVIEW_TEST_ENGINE_VERBOSE").is_some() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        }
        panic!("test engine: {stage} failed ({})", output.status);
    }
    output.stdout
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Cargo.lock").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("bins/mdv-app/Cargo.toml").display()
    );
    println!("cargo:rerun-if-env-changed=MDVIEW_TEST_ENGINE_VERBOSE");

    // Use Cargo from the selected compiler's toolchain, not the caller's CARGO
    // variable. This runs as a build input, not from a test process.
    let sysroot = run(
        "locate toolchain",
        Command::new(env::var_os("RUSTC").expect("Cargo did not provide RUSTC"))
            .args(["--print", "sysroot"]),
    );
    let cargo = PathBuf::from(String::from_utf8(sysroot).expect("toolchain path").trim())
        .join("bin")
        .join(format!("cargo{}", env::consts::EXE_SUFFIX));
    let target_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not provide OUT_DIR"))
        .join("engine");
    let target = env::var("TARGET").expect("Cargo did not provide TARGET");
    run(
        "build locked toolkit binary",
        Command::new(cargo)
            .current_dir(root)
            .env_remove("CARGO")
            .env_remove("CARGO_BUILD_BUILD_DIR")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .args([
                "build",
                "--locked",
                "--release",
                "-p",
                "mdshared-md-engine",
                "--bin",
                "mdview-engine",
                "--config",
                "build.rustc-wrapper=\"\"",
            ])
            .arg("--target")
            .arg(&target)
            .arg("--target-dir")
            .arg(&target_dir),
    );
    let suffix = if target.contains("-windows-") {
        ".exe"
    } else {
        ""
    };
    let engine = target_dir
        .join(target)
        .join("release")
        .join(format!("mdview-engine{suffix}"));
    assert!(engine.is_file(), "test engine: build output missing");
    println!("cargo:rerun-if-changed={}", engine.display());
    println!("cargo:rustc-env=MDVIEW_TEST_ENGINE={}", engine.display());
}

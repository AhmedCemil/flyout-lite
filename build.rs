use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let rc_file = manifest_dir.join("app.rc");
    let res_file = out_dir.join("app.res");

    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=flyout-lite.ico");

    let rc = locate_rc().expect(
        "Could not find rc.exe (Windows SDK resource compiler). \
         Build with the MSVC toolchain installed.",
    );

    let status = Command::new(&rc)
        .args(["/nologo", "/fo"])
        .arg(&res_file)
        .arg(&rc_file)
        .status()
        .expect("failed to invoke rc.exe");

    assert!(status.success(), "rc.exe failed");

    println!("cargo:rustc-link-arg-bins={}", res_file.display());
}

fn locate_rc() -> Option<PathBuf> {
    if let Ok(out) = Command::new("where").arg("rc.exe").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = s.lines().next() {
                let p = PathBuf::from(line.trim());
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    let roots = [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ];
    for root in roots {
        let root = PathBuf::from(root);
        if !root.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&root) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            versions.sort();
            versions.reverse();
            for v in versions {
                let candidate = v.join("x64").join("rc.exe");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

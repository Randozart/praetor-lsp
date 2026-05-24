use std::path::Path;
use std::process::Command;

/// Smoke test: verify praetor produces diagnostics on a known-bad file.
#[test]
fn praetor_produces_diagnostics() {
    let exe_path = find_praetor_binary().expect("praetor binary not found — build with cargo build first");
    let fixture = Path::new("test/fixtures/insecure.py");
    if !fixture.is_file() {
        eprintln!("fixture not found at {:?} — skipping smoke test", fixture);
        return;
    }

    let output = Command::new(&exe_path)
        .arg("report")
        .arg("--target")
        .arg(fixture.parent().unwrap())
        .output()
        .expect("failed to run praetor report");

    assert!(output.status.success(), "praetor report exited with error:\n{}",
        String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("insecure.py"),
        "expected insecure.py in report output but got:\n{}",
        stdout
    );
}

/// Find the praetor binary relative to the test binary or CWD.
fn find_praetor_binary() -> Option<std::path::PathBuf> {
    // Try relative to test binary (target/debug/praetor)
    let test_exe = std::env::current_exe().ok()?;
    let mut candidate = test_exe.parent()?.parent()?.join("praetor");
    if candidate.is_file() {
        return Some(candidate);
    }
    // Try CWD
    candidate = std::env::current_dir().ok()?.join("target").join("debug").join("praetor");
    if candidate.is_file() {
        return Some(candidate);
    }
    // Try PATH
    which("praetor")
}

#[cfg(unix)]
fn which(name: &str) -> Option<std::path::PathBuf> {
    let output = Command::new("which").arg(name).output().ok()?;
    if output.status.success() {
        Some(std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
    } else {
        None
    }
}

#[cfg(not(unix))]
fn which(_name: &str) -> Option<std::path::PathBuf> {
    None
}

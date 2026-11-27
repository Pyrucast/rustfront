//! Runs the `mtest-rs` binary. The behaviours of `tests/behaviours` are
//! compiled by `mfront`: without it in the `PATH`, the test fails.

use std::path::Path;
use std::process::Command;

#[test]
fn elastic_tension_matches_the_analytical_solution() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mtest-rs");
    std::fs::create_dir_all(&dir).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("mfront")
        .args(["--obuild", "--interface=generic"])
        .arg(root.join("tests/behaviours/Elasticity.mfront"))
        .current_dir(&dir)
        .output()
        .unwrap_or_else(|e| {
            panic!("cannot run `mfront` ({e}): this test needs MFront (TFEL ≥ 5.1.0) in the PATH")
        });
    assert!(output.status.success(), "mfront failed: {output:?}");

    let case = root.join("mtest-rs/cases/elasticity-tension.mtest");
    let run = Command::new(env!("CARGO_BIN_EXE_mtest-rs"))
        .arg(&case)
        .args(["--output", "tension.res"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run.status.success(), "mtest-rs failed: {run:?}");

    let text = std::fs::read_to_string(dir.join("tension.res")).unwrap();
    let rows: Vec<Vec<f64>> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect()
        })
        .collect();
    assert_eq!(rows.len(), 11);
    let (young, poisson) = (200e9, 0.3);
    for row in rows {
        // Columns: time, 6 strains, 6 stresses, von Mises stress, energies.
        let (t, exx, sxx) = (row[0], row[1], row[7]);
        let close = |found: f64, expected: f64, scale: f64| {
            assert!(
                (found - expected).abs() <= 1e-9 * scale,
                "t = {t}: {found} ≠ {expected}"
            );
        };
        close(exx, 1e-3 * t, 1e-3);
        close(row[2], -poisson * exx, 1e-3);
        close(row[3], -poisson * exx, 1e-3);
        close(sxx, young * exx, young * 1e-3);
        for free in &row[8..13] {
            close(*free, 0.0, young * 1e-3);
        }
        close(row[13], sxx, young * 1e-3);
    }
}

#[test]
fn mistakes_in_the_input_are_reported_with_their_line() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mtest-rs");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("mistake.mtest");
    std::fs::write(
        &file,
        "@Behaviour<generic> 'src/libBehaviour.so' 'Elasticity';\n@Tims {0, 1};\n",
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_mtest-rs"))
        .arg(&file)
        .output()
        .unwrap();
    assert!(!run.status.success());
    let error = String::from_utf8_lossy(&run.stderr);
    assert!(
        error.contains("line 2: unknown or unsupported keyword `@Tims`"),
        "{error}"
    );
}

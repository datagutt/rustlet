use std::env;
use std::path::PathBuf;

use rustlet_compat::{load_manifest, pixlet_available, run_case, CaseOutcome, ComparisonPolicy};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

#[test]
fn compat_manifest_cases() {
    let workspace_root = workspace_root();
    if !pixlet_available(&workspace_root) {
        eprintln!("skipping compat tests: pixlet oracle is unavailable");
        return;
    }

    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = load_manifest(&crate_root).expect("compat manifest should parse");
    let filter = env::var("COMPAT_CASE").ok();
    let mut known_diffs = Vec::new();
    let mut ran = 0usize;

    for case in manifest.cases {
        if let Some(filter) = &filter {
            if !case.id.contains(filter) {
                continue;
            }
        }

        let report = run_case(&workspace_root, &crate_root, case.clone())
            .unwrap_or_else(|err| panic!("compat case {} failed: {err:#}", case.id));
        match &report.outcome {
            CaseOutcome::Skipped(reason) => eprintln!("compat skip {}: {}", report.case.id, reason),
            CaseOutcome::KnownDiff(failure) => {
                eprintln!("compat known diff {}: {}", report.case.id, failure.message);
                known_diffs.push(report.case.id.clone());
            }
            CaseOutcome::ExactMatch => {}
        }
        if report.case.policy != ComparisonPolicy::Skip {
            ran += 1;
        }
    }

    assert!(ran > 0, "compat test did not run any cases");
    if env::var_os("COMPAT_FAIL_ON_KNOWN_DIFF").is_some() {
        assert!(
            known_diffs.is_empty(),
            "known diff cases were present: {}",
            known_diffs.join(", ")
        );
    }
}

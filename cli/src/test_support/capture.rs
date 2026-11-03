//! Capture the real terminal boundary without replacing production renderers or stdout.
#![allow(clippy::panic)]

use std::fs::File;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD: &str = "TRUST_OUTPUT_CONTRACT_CHILD";
const START: &str = "--- trust output contract begins ---";
const END: &str = "--- trust output contract ends ---";

pub(crate) fn assert_stdout(
    module: &str,
    name: &str,
    exercise: impl FnOnce(),
    check: impl FnOnce(&str),
) {
    let module = module.split_once("::").unwrap().1;
    let test = format!("{module}::{name}");
    if std::env::var(CHILD).as_deref() == Ok(test.as_str()) {
        println!("{START}");
        exercise();
        println!("{END}");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let out = directory.path().join("stdout");
    let err = directory.path().join("stderr");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test, "--nocapture", "--test-threads=1"])
        .env(CHILD, &test)
        .env("TRUST_DB_URL", directory.path().join("financial.db"))
        .env("TRUST_DISABLE_KEYCHAIN", "1")
        .env("TRUST_IBKR_LIVE_E2E", "0")
        .env_remove("TRUST_IBKR_ALLOW_PAPER_MUTATIONS")
        .stdin(Stdio::null())
        .stdout(File::create(&out).unwrap())
        .stderr(File::create(&err).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if deadline.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Terminal contract timed out: {test}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let output = std::fs::read_to_string(out).unwrap();
    let errors = std::fs::read_to_string(err).unwrap();
    assert!(status.success(), "{test}\n{output}\n{errors}");
    let output = output
        .split_once(START)
        .and_then(|(_, body)| body.split_once(END).map(|(body, _)| body))
        .expect("the selected scenario must execute completely, not zero tests");
    check(output);
}

/// Isolate one actual command result from a multi-command terminal scenario.
pub(crate) fn section<'a>(output: &'a str, label: &str) -> &'a str {
    let begin = format!("BEGIN:{label}\n");
    let end = format!("END:{label}\n");
    assert_eq!(
        output.matches(&begin).count(),
        1,
        "missing/duplicate {label}: {output}"
    );
    assert_eq!(
        output.matches(&end).count(),
        1,
        "incomplete {label}: {output}"
    );
    output
        .split_once(&begin)
        .unwrap()
        .1
        .split_once(&end)
        .unwrap()
        .0
}

pub(crate) fn json_section(output: &str, label: &str) -> serde_json::Value {
    serde_json::from_str(section(output, label))
        .expect("command output must be one valid JSON document")
}

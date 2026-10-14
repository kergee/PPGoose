use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn call_bytes(bytes: &[u8]) -> (bool, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ppgoose-cli"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let result = child.wait_with_output().unwrap();
    (
        result.status.success(),
        serde_json::from_slice(&result.stdout).unwrap(),
    )
}

fn call(value: Value) -> (bool, Value) {
    call_bytes(&serde_json::to_vec(&value).unwrap())
}

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let input = dir.join("中文 空格 🪿.png");
    image::RgbaImage::from_fn(64, 32, |x, y| image::Rgba([x as u8, y as u8, 127, 255]))
        .save(&input)
        .unwrap();
    input
}

#[test]
fn partial_options_default_to_separate_output_and_collisions_are_numbered() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let original = std::fs::read(&input).unwrap();
    let request = json!({"protocolVersion":1,"operation":"compress",
        "inputPath":input,"dataDirectory":dir.path().join("state"),"options":{"quality":80}});
    let (success, first) = call(request.clone());
    assert!(success, "{first}");
    assert_eq!(first["ok"], true);
    assert_eq!(first["result"]["overwritten"], false);
    assert_eq!(std::fs::read(&input).unwrap(), original);
    let (success, second) = call(request);
    assert!(success, "{second}");
    assert_ne!(
        first["result"]["outputPath"],
        second["result"]["outputPath"]
    );
    assert!(image::open(first["result"]["outputPath"].as_str().unwrap()).is_ok());
}

#[test]
fn overwrite_list_and_restore_work_across_cli_processes() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let original = std::fs::read(&input).unwrap();
    let state = dir.path().join("state");
    let (success, result) = call(json!({"protocolVersion":1,"operation":"compress",
        "inputPath":input,"dataDirectory":state,"options":{"outputMode":"overwrite"}}));
    assert!(success, "{result}");
    assert_eq!(result["result"]["overwritten"], true);
    let id = &result["result"]["recordId"];
    let (success, list) =
        call(json!({"protocolVersion":1,"operation":"listHistory","dataDirectory":state}));
    assert!(success, "{list}");
    assert_eq!(list["result"][0]["id"], *id);
    let (success, restored) = call(
        json!({"protocolVersion":1,"operation":"restore","dataDirectory":state,"recordId":id}),
    );
    assert!(success, "{restored}");
    assert_eq!(restored["result"]["state"], "restored");
    assert_eq!(std::fs::read(&input).unwrap(), original);
}

#[test]
fn malformed_requests_and_invalid_options_return_json_errors_without_files() {
    let (success, response) = call_bytes(b"broken JSON");
    assert!(!success && response["ok"] == false);
    assert_eq!(response["error"]["code"], "invalidRequest");
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let state = dir.path().join("state");
    for options in [
        json!({"quality":101}),
        json!({"qualty":20}),
        json!({"suffix":"../escape"}),
    ] {
        let (success, response) = call(json!({"protocolVersion":1,"operation":"compress",
            "inputPath":input,"dataDirectory":state,"options":options}));
        assert!(!success && response["ok"] == false);
        assert!(!state.exists());
        assert!(!dir.path().join("compressed").exists());
    }
    let (success, _) =
        call(json!({"protocolVersion":2,"operation":"listHistory","dataDirectory":state}));
    assert!(!success && !state.exists());
    let (success, _) =
        call(json!({"protocolVersion":1,"operation":"listHistory","dataDirectory":"relative"}));
    assert!(!success);
}

#[test]
fn utf8_bom_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = b"\xef\xbb\xbf".to_vec();
    request.extend(
        serde_json::to_vec(&json!({"protocolVersion":1,"operation":"listHistory",
        "dataDirectory":dir.path().join("state")}))
        .unwrap(),
    );
    assert!(call_bytes(&request).0);
}

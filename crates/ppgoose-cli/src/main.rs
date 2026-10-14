use anyhow::{ensure, Context, Result};
use ppgoose_core::{compress_file, storage::Store, CompressOptions, OutputMode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::Path;

const PROTOCOL_VERSION: u8 = 1;
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    protocol_version: u8,
    operation: Operation,
    data_directory: String,
    input_path: Option<String>,
    record_id: Option<String>,
    options: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Operation {
    Compress,
    ListHistory,
    Restore,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    protocol_version: u8,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

fn absolute_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    ensure!(path.is_absolute(), "必须使用绝对路径: {value}");
    Ok(path)
}

fn execute(request: Request) -> Result<Value> {
    ensure!(
        request.protocol_version == PROTOCOL_VERSION,
        "不支持的 protocolVersion"
    );
    let root = absolute_path(&request.data_directory)?;
    let store = Store::new(root.to_owned());
    match request.operation {
        Operation::Compress => {
            ensure!(request.record_id.is_none(), "compress 不接受 recordId");
            let input = request.input_path.context("compress 需要 inputPath")?;
            let input = absolute_path(&input)?;
            // CMS defaults to a no-clobber output. A partial JSON options
            // object must not inherit the desktop's overwrite default.
            let mut options = request.options.unwrap_or_else(|| json!({}));
            let object = options
                .as_object_mut()
                .context("options 必须是 JSON 对象")?;
            object.entry("outputMode").or_insert(json!("subfolder"));
            let options: CompressOptions = serde_json::from_value(options)?;
            ensure!(
                options.output_mode != OutputMode::Custom || options.custom_dir.is_some(),
                "custom 输出需要 customDir"
            );
            Ok(serde_json::to_value(compress_file(
                input, &options, &store,
            )?)?)
        }
        Operation::ListHistory => {
            ensure!(
                request.input_path.is_none()
                    && request.options.is_none()
                    && request.record_id.is_none(),
                "listHistory 只接受 dataDirectory"
            );
            Ok(serde_json::to_value(store.records()?)?)
        }
        Operation::Restore => {
            ensure!(
                request.input_path.is_none() && request.options.is_none(),
                "restore 不接受 inputPath/options"
            );
            let id = request.record_id.context("restore 需要 recordId")?;
            Ok(serde_json::to_value(store.restore(&id)?)?)
        }
    }
}

fn run() -> Response {
    let request = (|| -> Result<Request> {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(MAX_REQUEST_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_REQUEST_BYTES,
            "JSON 请求超过 1 MiB"
        );
        // PowerShell-generated UTF-8 files can carry a BOM.
        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
        Ok(serde_json::from_slice(bytes)?)
    })();
    let outcome = match request {
        Ok(request) => execute(request).map_err(|error| ("processingFailed", error)),
        Err(error) => Err(("invalidRequest", error)),
    };
    match outcome {
        Ok(result) => Response {
            protocol_version: PROTOCOL_VERSION,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err((code, error)) => Response {
            protocol_version: PROTOCOL_VERSION,
            ok: false,
            result: None,
            error: Some(json!({ "code": code, "message": format!("{error:#}") })),
        },
    }
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("ppgoose-cli {} protocol=1", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.len() == 1 && args[0] == "--help" {
        println!("ppgoose-cli: UTF-8 JSON request on stdin (close stdin after writing); one JSON response on stdout.\nOperations: compress, listHistory, restore. protocolVersion=1; dataDirectory must be absolute.\nExit codes: 0=success, 1=request/processing failure, 2=output/argument failure.\nSee docs/CMS-独立压缩核心.txt.");
        return;
    }
    if !args.is_empty() {
        eprintln!("Use stdin JSON; see --help.");
        std::process::exit(2);
    }
    let response = run();
    let ok = response.ok;
    let mut stdout = std::io::stdout().lock();
    if serde_json::to_writer(&mut stdout, &response).is_err()
        || stdout.write_all(b"\n").is_err()
        || stdout.flush().is_err()
    {
        std::process::exit(2);
    }
    if !ok {
        std::process::exit(1);
    }
}

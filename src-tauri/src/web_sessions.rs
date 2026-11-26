//! HTTP adapters for the existing session parsers, cache, search and deletion.
use serde::Deserialize;
use serde_json::{json, Value};

use crate::session_manager::{self, cache::chunk_ranges, content, model::TranscriptChunk, search};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionSource {
    pub provider_id: String,
    pub source_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRequest {
    pub provider_id: String,
    pub source_path: String,
    pub image: session_manager::model::ImageRef,
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|_| format!("Invalid session argument: {key}"))
}

pub fn dispatch(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "list_sessions" => Ok(json!(session_manager::scan_sessions())),
        "get_session_messages" => {
            let source: SessionSource =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let loaded =
                session_manager::load_transcript(&source.provider_id, &source.source_path)?;
            Ok(json!(loaded.transcript.messages))
        }
        "get_session_block_content" => {
            let source = content::validate_source(
                &arg::<String>(args, "providerId")?,
                &arg::<String>(args, "sourcePath")?,
            )?;
            let full = content::resolve_content_ref(&source, &arg(args, "contentRef")?)?;
            Ok(json!(content::paginate(
                &full,
                arg(args, "offset")?,
                arg(args, "limit")?
            )))
        }
        "search_session_content" => {
            let request = search::begin_search(
                arg::<Option<String>>(args, "requestId")?
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            );
            search::search(
                &arg::<String>(args, "query")?,
                arg(args, "providerIds")?,
                arg::<Option<usize>>(args, "limit")?.unwrap_or(search::DEFAULT_LIMIT),
                request,
            )
            .map(|value| json!(value))
        }
        "cancel_session_content_search" => {
            search::cancel_search(&arg::<String>(args, "requestId")?);
            Ok(Value::Null)
        }
        "clear_session_index" => search::clear().map(|()| json!(true)),
        "delete_session" => session_manager::delete_session(
            &arg::<String>(args, "providerId")?,
            &arg::<String>(args, "sessionId")?,
            &arg::<String>(args, "sourcePath")?,
        )
        .map(|value| json!(value)),
        "delete_sessions" => Ok(json!(session_manager::delete_sessions(&arg::<
            Vec<session_manager::DeleteSessionRequest>,
        >(
            args, "items"
        )?))),
        _ => Err(format!("Web 入口尚不支持此操作: {command}")),
    }
}

pub fn sync_index(
    notify: impl Fn(search::IndexStatus) + Send + Sync + 'static,
) -> search::IndexStatus {
    search::start_sync(notify)
}

/// Called on a blocking worker. The HTTP transport applies bounded backpressure.
pub fn stream_messages(
    source: SessionSource,
    mut send: impl FnMut(Vec<u8>) -> Result<(), String>,
) -> Result<(), String> {
    let loaded = session_manager::load_transcript(&source.provider_id, &source.source_path)?;
    let transcript = loaded.transcript;
    let mut emit = |chunk: TranscriptChunk<'_>| {
        let mut bytes = serde_json::to_vec(&chunk).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        send(bytes)
    };
    emit(TranscriptChunk::Header {
        total: transcript.messages.len(),
        turns: &transcript.turns,
        cached: loaded.cached,
        parse_ms: loaded.parse_ms,
    })?;
    for (start, end) in chunk_ranges(&transcript.message_bytes) {
        emit(TranscriptChunk::Messages {
            start,
            messages: &transcript.messages[start..end],
        })?;
    }
    emit(TranscriptChunk::Done {
        payload_bytes: transcript.approx_bytes as u64,
    })
}

pub fn image(request: ImageRequest) -> Result<Vec<u8>, String> {
    if !matches!(
        request.image.media_type.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/bmp"
    ) {
        return Err("Unsupported session image type".into());
    }
    let source = content::validate_source(&request.provider_id, &request.source_path)?;
    content::load_image(&source, &request.image)
}

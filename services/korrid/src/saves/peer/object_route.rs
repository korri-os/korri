//! Loaded as a child of peer_rpc so both routes use its one authentication and
//! replay boundary. See peer-wiring.patch; this file defines no new envelope.
use super::*;
use crate::saves::{peer::PeerSaveError, SaveError};
use axum::{
    body::Body,
    http::{header, HeaderMap},
};
use std::io::{Read, Seek, SeekFrom};

pub(super) async fn object_route(
    State(state): State<PeerRpcServer>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let event = match std::str::from_utf8(&body) {
        Ok(event) => event,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let authenticated = match state.authenticate(event, (state.clock)(), true).await {
        Ok(authenticated) => authenticated,
        Err(PeerRpcError::Replay) => return StatusCode::CONFLICT.into_response(),
        Err(PeerRpcError::Unauthorized) => return StatusCode::FORBIDDEN.into_response(),
        Err(PeerRpcError::Time) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let release = matches!(
        &authenticated.envelope.request,
        RpcRequest::ReleaseObject(_)
    );
    let file = match authenticated.envelope.request {
        RpcRequest::ReleaseObject(request) => match crate::netplay::holder::object(
            &state.app,
            &authenticated.context,
            &request.release_key,
        )
        .await
        {
            Ok(file) => file,
            Err(_) => return StatusCode::FORBIDDEN.into_response(),
        },
        RpcRequest::SaveObject(request) => {
            let Some(service) = &state.app.saves else {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            };
            match service.object(&authenticated.context, request.sha256).await {
                Ok(file) => file,
                Err(PeerSaveError::Unauthorized) => return StatusCode::FORBIDDEN.into_response(),
                Err(PeerSaveError::Protocol) => return StatusCode::BAD_REQUEST.into_response(),
                Err(PeerSaveError::Save(SaveError::Missing)) => {
                    return StatusCode::NOT_FOUND.into_response()
                }
                Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
            }
        }
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let bytes = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let range = match headers.get(header::RANGE) {
        None => Ok((0, bytes, false)),
        Some(value) if headers.get_all(header::RANGE).iter().count() == 1 => value
            .to_str()
            .ok()
            .and_then(|value| byte_range(value, bytes))
            .map(|(start, length)| (start, length, true))
            .ok_or(()),
        _ => Err(()),
    };
    let (offset, length, partial) = match range {
        Ok(range) => range,
        Err(()) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{bytes}"))],
            )
                .into_response()
        }
    };
    use std::os::fd::AsRawFd;
    let release_name = release
        .then(|| {
            std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
                .ok()
                .and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
        })
        .flatten();
    let stream = futures::stream::try_unfold(
        (file, offset, length),
        |(mut file, offset, remaining)| async move {
            if remaining == 0 {
                return Ok(None);
            }
            tokio::task::spawn_blocking(move || {
                file.seek(SeekFrom::Start(offset))?;
                let mut chunk = vec![0; remaining.min(64 * 1024) as usize];
                let read = file.read(&mut chunk)?;
                if read == 0 {
                    return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
                }
                chunk.truncate(read);
                Ok(Some((
                    chunk,
                    (file, offset + read as u64, remaining - read as u64),
                )))
            })
            .await
            .map_err(std::io::Error::other)?
        },
    );
    let mut response = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_LENGTH, length);
    if let Some(name) = release_name {
        response = response.header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename*=UTF-8''{}",
                crate::host::play_log::encode_uri_component(&name)
            ),
        );
    }
    if partial {
        response = response.header(
            header::CONTENT_RANGE,
            format!("bytes {offset}-{}/{bytes}", offset + length - 1),
        );
    }
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn byte_range(value: &str, bytes: u64) -> Option<(u64, u64)> {
    if bytes == 0 {
        return None;
    }
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    if !start.bytes().all(|byte| byte.is_ascii_digit())
        || !end.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        let length = suffix.min(bytes);
        return Some((bytes - length, length));
    }
    let start = start.parse::<u64>().ok()?;
    let end = if end.is_empty() {
        bytes - 1
    } else {
        end.parse::<u64>().ok()?.min(bytes - 1)
    };
    if start > end || start >= bytes {
        return None;
    }
    Some((start, end - start + 1))
}

#[cfg(test)]
mod range_tests {
    use super::byte_range;
    #[test]
    fn single_closed_open_and_suffix_ranges() {
        assert_eq!(byte_range("bytes=2-4", 10), Some((2, 3)));
        assert_eq!(byte_range("bytes=2-", 10), Some((2, 8)));
        assert_eq!(byte_range("bytes=-3", 10), Some((7, 3)));
        assert_eq!(byte_range("bytes=0-99", 10), Some((0, 10)));
        for value in [
            "bytes=10-",
            "bytes=4-2",
            "bytes=-0",
            "bytes=1-2,4-5",
            "bytes=-",
            "items=1-2",
            "bytes=+1-2",
        ] {
            assert_eq!(byte_range(value, 10), None, "{value}");
        }
        assert_eq!(byte_range("bytes=0-", 0), None);
    }
}

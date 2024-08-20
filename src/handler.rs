// Copyright 2024 Wladimir Palant
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Handler for the `request_filter` phase.

use async_trait::async_trait;
use http::{method::Method, status::StatusCode};
use log::{debug, info, warn};
use pingora::{Error, ErrorType};
use crate::session_wrapper::SessionWrapper;
use crate::standard_response::{error_response, redirect_response};
use crate::request_filter::{RequestFilter, RequestFilterResult};
use std::io::ErrorKind;
use std::path::PathBuf;

use crate::compression::Compression;
use crate::configuration::StaticFilesConf;
use crate::file_writer::file_response;
use crate::metadata::Metadata;
use crate::mime_matcher::MimeMatcher;
use crate::path::{path_to_uri, resolve_uri};
use crate::range::{extract_range, Range};
use crate::CompressionAlgorithm;

const DEFAULT_TEXT_TYPES: &[&str] = &[
    "text/*",
    "*+xml",
    "*+json",
    "application/javascript",
    "application/json",
    "application/json5",
];

/// Static Files module handler
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticFilesHandler {
    root: Option<PathBuf>,
    canonicalize_uri: bool,
    index_file: Vec<String>,
    page_404: Option<String>,
    precompressed: Vec<CompressionAlgorithm>,
    declare_charset: String,
    declare_charset_matcher: MimeMatcher,
}

#[async_trait]
impl RequestFilter for StaticFilesHandler {
    type Conf = StaticFilesConf;

    type CTX = ();

    fn new_ctx() -> Self::CTX {}

    async fn request_filter(
        &self,
        session: &mut impl SessionWrapper,
        _ctx: &mut Self::CTX,
    ) -> Result<RequestFilterResult, Box<Error>> {
        let root = if let Some(root) = self.root.as_ref() {
            root
        } else {
            debug!("received request but static files handler is not configured, ignoring");
            return Ok(RequestFilterResult::Unhandled);
        };

        let uri = session.uri();
        debug!("received URI path {}", uri.path());

        let (mut path, not_found) = match resolve_uri(uri.path(), root) {
            Ok(path) => (path, false),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                debug!("canonicalizing resulted in NotFound error");

                let path = self.page_404.as_ref().and_then(|page_404| {
                    debug!("error page is {page_404}");
                    match resolve_uri(page_404, root) {
                        Ok(path) => Some(path),
                        Err(err) => {
                            warn!("Failed resolving error page {page_404}: {err}");
                            None
                        }
                    }
                });

                if let Some(path) = path {
                    (path, true)
                } else {
                    error_response(session, StatusCode::NOT_FOUND).await?;
                    return Ok(RequestFilterResult::ResponseSent);
                }
            }
            Err(err) => {
                let status = match err.kind() {
                    ErrorKind::InvalidInput => {
                        warn!("rejecting invalid path {}", uri.path());
                        StatusCode::BAD_REQUEST
                    }
                    ErrorKind::InvalidData => {
                        warn!("Requested path outside root directory: {}", uri.path());
                        StatusCode::BAD_REQUEST
                    }
                    ErrorKind::PermissionDenied => {
                        debug!("canonicalizing resulted in PermissionDenied error");
                        StatusCode::FORBIDDEN
                    }
                    _ => {
                        warn!("failed canonicalizing the path {}: {err}", uri.path());
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                };
                error_response(session, status).await?;
                return Ok(RequestFilterResult::ResponseSent);
            }
        };

        debug!("translated into file path {path:?}");

        if self.canonicalize_uri && !not_found {
            if let Some(mut canonical) = path_to_uri(&path, root) {
                if canonical != uri.path() {
                    if let Some(query) = uri.query() {
                        canonical.push('?');
                        canonical.push_str(query);
                    }

                    if let Some(prefix) = session
                        .original_uri()
                        .path()
                        .strip_suffix(uri.path())
                        .filter(|p| !p.is_empty())
                    {
                        // A prefix has been removed from the original URI, insert it for the
                        // redirect.
                        canonical.insert_str(0, prefix);
                    }
                    info!("redirecting to canonical URI: {canonical}");
                    redirect_response(session, StatusCode::PERMANENT_REDIRECT, &canonical).await?;
                    return Ok(RequestFilterResult::ResponseSent);
                }
            }
        }

        if path.is_dir() {
            for filename in &self.index_file {
                let candidate = path.join(filename);
                if candidate.is_file() {
                    debug!("using directory index file {filename}");
                    path = candidate;
                }
            }
        }

        info!("successfully resolved request path: {path:?}");

        match session.req_header().method {
            Method::GET | Method::HEAD => {
                // Allowed
            }
            _ => {
                warn!("Denying method {}", session.req_header().method);
                error_response(session, StatusCode::METHOD_NOT_ALLOWED).await?;
                return Ok(RequestFilterResult::ResponseSent);
            }
        }

        let mut compression = Compression::new(session, &self.precompressed);

        let (path, orig_path) =
            if let Some(precompressed_path) = compression.rewrite_path(session, &path) {
                (precompressed_path, Some(path))
            } else {
                (path, None)
            };

        let meta = match Metadata::from_path(&path, orig_path.as_ref()) {
            Ok(meta) => meta,
            Err(err) if err.kind() == ErrorKind::InvalidInput => {
                warn!("Path {path:?} is not a regular file, denying access");
                error_response(session, StatusCode::FORBIDDEN).await?;
                return Ok(RequestFilterResult::ResponseSent);
            }
            Err(err) => {
                warn!("failed retrieving metadata for path {path:?}: {err}");
                error_response(session, StatusCode::INTERNAL_SERVER_ERROR).await?;
                return Ok(RequestFilterResult::ResponseSent);
            }
        };

        if meta.has_failed_precondition(session) {
            debug!("If-Match/If-Unmodified-Since precondition failed");
            let header = meta.to_custom_header(StatusCode::PRECONDITION_FAILED)?;
            let header = compression.transform_header(session, header)?;
            session.write_response_header(header, true).await?;
            return Ok(RequestFilterResult::ResponseSent);
        }

        if meta.is_not_modified(session) {
            debug!("If-None-Match/If-Modified-Since check resulted in Not Modified");
            let header = meta.to_custom_header(StatusCode::NOT_MODIFIED)?;
            let header = compression.transform_header(session, header)?;
            session.write_response_header(header, true).await?;
            return Ok(RequestFilterResult::ResponseSent);
        }

        let charset = if self.declare_charset_matcher.matches(&meta.mime) {
            Some(self.declare_charset.as_str())
        } else {
            None
        };

        let (mut header, start, end) = match extract_range(session, &meta) {
            Some(Range::Valid(start, end)) => {
                debug!("bytes range requested: {start}-{end}");
                let header = meta.to_partial_content_header(charset, start, end)?;
                let header = compression.transform_header(session, header)?;
                (header, start, end)
            }
            Some(Range::OutOfBounds) => {
                debug!("requested bytes range is out of bounds");
                let header = meta.to_not_satisfiable_header(charset)?;
                let header = compression.transform_header(session, header)?;
                session.write_response_header(header, true).await?;
                return Ok(RequestFilterResult::ResponseSent);
            }
            None => {
                // Range is either missing or cannot be parsed, produce the entire file.
                let header = meta.to_response_header(charset)?;
                let header = compression.transform_header(session, header)?;
                (header, 0, meta.size - 1)
            }
        };

        if not_found {
            header.set_status(StatusCode::NOT_FOUND)?;
        }

        let send_body = session.req_header().method != Method::HEAD;
        session.write_response_header(header, !send_body).await?;

        if send_body {
            // sendfile would be nice but not currently possible within pingora-proxy (see
            // https://github.com/cloudflare/pingora/issues/160)
            file_response(session, &path, start, end).await?;
        }
        Ok(RequestFilterResult::ResponseSent)
    }
}

impl TryFrom<StaticFilesConf> for StaticFilesHandler {
    type Error = Box<Error>;

    fn try_from(conf: StaticFilesConf) -> Result<Self, Self::Error> {
        let root = if let Some(root) = conf.root {
            Some(root.canonicalize().map_err(|err| {
                Error::because(
                    ErrorType::InternalError,
                    format!("Failed accessing root path {:?}", root),
                    err,
                )
            })?)
        } else {
            None
        };

        let mut declare_charset_matcher = MimeMatcher::new();
        if !conf.declare_charset_types.is_empty() {
            for mime in conf.declare_charset_types {
                declare_charset_matcher.add(mime);
            }
        } else {
            for mime in DEFAULT_TEXT_TYPES {
                declare_charset_matcher.add((*mime).try_into().unwrap());
            }
        }

        Ok(Self {
            root,
            canonicalize_uri: conf.canonicalize_uri,
            index_file: conf.index_file.into(),
            page_404: conf.page_404,
            precompressed: conf.precompressed.into(),
            declare_charset: conf.declare_charset,
            declare_charset_matcher,
        })
    }
}

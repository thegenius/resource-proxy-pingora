use http::{header, method::Method, status::StatusCode};
use maud::{html, DOCTYPE};

use pingora::{Error};
use pingora::http::ResponseHeader;
// use crate::session_wrapper::SessionWrapper;
use pingora::proxy::Session;

/// Produces the text of a standard response page for the given status code.
pub fn response_text(status: StatusCode) -> String {
    let status_str = status.as_str();
    let reason = status.canonical_reason().unwrap_or("");
    let pre_escaped = html! {
        (DOCTYPE)
        html {
            head {
                title {
                    (status_str) " " (reason)
                }
            }

            body {
                center {
                    h1 {
                        (status_str) " " (reason)
                    }
                }
            }
        }
    };
    pre_escaped.into_string()
}

async fn response(
    session: &mut Session,
    status: StatusCode,
    location: Option<&str>,
    cookie: Option<&str>,
) -> Result<(), Box<Error>> {
    let text = response_text(status);

    let mut header = ResponseHeader::build(status, Some(4))?;
    header.append_header(header::CONTENT_LENGTH, text.len().to_string())?;
    header.append_header(header::CONTENT_TYPE, "text/html;charset=utf-8")?;
    if let Some(location) = location {
        header.append_header(header::LOCATION, location)?;
    }
    if let Some(cookie) = cookie {
        header.append_header(header::SET_COOKIE, cookie)?;
    }

    let send_body = session.req_header().method != Method::HEAD;
    session
        .write_response_header(Box::new(header), !send_body)
        .await?;

    if send_body {
        session.write_response_body(Some(text.into()), true).await?;
    }

    Ok(())
}

/// Responds with a standard error page for the given status code.
pub async fn error_response(
    session: &mut Session,
    status: StatusCode,
) -> Result<(), Box<Error>> {
    response(session, status, None, None).await
}

/// Responds with a redirect to the given location.
pub async fn redirect_response(
    session: &mut Session,
    status: StatusCode,
    location: &str,
) -> Result<(), Box<Error>> {
    response(session, status, Some(location), None).await
}

/// Responds with a redirect to the given location and setting a cookie.
pub async fn redirect_response_with_cookie(
    session: &mut Session,
    status: StatusCode,
    location: &str,
    cookie: &str,
) -> Result<(), Box<Error>> {
    response(session, status, Some(location), Some(cookie)).await
}
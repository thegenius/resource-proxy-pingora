// use async_trait::async_trait;
// use bytes::{Bytes, BytesMut};
// use http::{header, Extensions, Uri};
// use once_cell::sync::OnceCell;
// pub use pingora::http::{IntoCaseHeaderName, RequestHeader, ResponseHeader};
// pub use pingora::modules::http::compression::{ResponseCompression, ResponseCompressionBuilder};
// pub use pingora::modules::http::{HttpModule, HttpModuleBuilder, HttpModules};
// pub use pingora::protocols::http::compression::Algorithm as CompressionAlgorithm;
// pub use pingora::protocols::l4::socket::SocketAddr;
// pub use pingora::proxy::{http_proxy_service, ProxyHttp, Session};
// pub use pingora::server::configuration::{Opt as ServerOpt, ServerConf};
// pub use pingora::server::Server;
// pub use pingora::upstreams::peer::HttpPeer;
// pub use pingora::{Error, ErrorType};
// use std::borrow::Cow;
// use std::io::{Cursor, Seek, SeekFrom, Write};
// use std::ops::{Deref, DerefMut};
// use std::sync::Arc;
// #[async_trait]
// pub trait SessionWrapper: Send + Deref<Target = Session> + DerefMut {
//     /// Attempts to determine the request host if one was specified.
//     fn host(&self) -> Option<Cow<'_, str>>
//     where
//         Self: Sized,
//     {
//         fn host_from_header(session: &impl SessionWrapper) -> Option<Cow<'_, str>> {
//             let host = session.get_header(header::HOST)?;
//             host.to_str().ok().map(|h| h.into())
//         }
//
//         fn host_from_uri(session: &impl SessionWrapper) -> Option<Cow<'_, str>> {
//             let uri = session.uri();
//             let host = uri.host()?;
//             if let Some(port) = uri.port() {
//                 let mut host = host.to_owned();
//                 host.push(':');
//                 host.push_str(port.as_str());
//                 Some(host.into())
//             } else {
//                 Some(host.into())
//             }
//         }
//
//         host_from_header(self).or_else(|| host_from_uri(self))
//     }
//
//     /// Overwrites the client address for this connection.
//     fn set_client_addr(&mut self, addr: SocketAddr) {
//         if let Some(digest) = self.digest_mut() {
//             // Existing SocketDigest is behind an Arc reference and cannot be changed, create a new
//             // one.
//             let mut socket_digest = pingora::protocols::SocketDigest::from_raw_fd(0);
//             socket_digest.peer_addr = OnceCell::new();
//             let _ = socket_digest.peer_addr.set(Some(addr));
//             socket_digest.local_addr = OnceCell::new();
//             let _ = socket_digest.local_addr.set(
//                 digest
//                     .socket_digest
//                     .as_ref()
//                     .and_then(|digest| digest.local_addr().cloned()),
//             );
//             digest.socket_digest = Some(Arc::new(socket_digest));
//         }
//     }
//
//     /// Returns a reference to the associated extensions.
//     fn extensions(&self) -> &Extensions;
//
//     /// Returns a mutable reference to the associated extensions.
//     fn extensions_mut(&mut self) -> &mut Extensions;
//
//     /// Returns the request URI.
//     ///
//     /// This might not be the original request URI but manipulated by Rewrite module for example.
//     fn uri(&self) -> &Uri {
//         &self.req_header().uri
//     }
//
//     /// Changes the request URI and saves the original URI.
//     ///
//     /// This method should be used instead of manipulating the request URI in the header.
//     fn set_uri(&mut self, uri: Uri) {
//         let current_uri = OriginalUri(self.uri().clone());
//         self.extensions_mut().get_or_insert(current_uri);
//         self.req_header_mut().set_uri(uri);
//     }
//
//     /// Returns the original URI of the request which might have been modified by e.g.
//     /// by Rewrite module afterwards.
//     fn original_uri(&self) -> &Uri {
//         if let Some(OriginalUri(uri)) = self.extensions().get() {
//             uri
//         } else {
//             self.uri()
//         }
//     }
//
//     /// Returns the name of the authorized user if any
//     fn remote_user(&self) -> Option<&str> {
//         if let Some(RemoteUser(remote_user)) = self.extensions().get() {
//             Some(remote_user)
//         } else {
//             None
//         }
//     }
//
//     /// Sets the name of the authorized user
//     fn set_remote_user(&mut self, remote_user: String) {
//         self.extensions_mut().insert(RemoteUser(remote_user));
//     }
//
//     /// See [`Session::response_written`](pingora::protocols::http::server::Session::response_written)
//     fn response_written(&self) -> Option<&ResponseHeader> {
//         self.deref().response_written()
//     }
//
//     /// See [`Session::write_response_body`](pingora::protocols::http::server::Session::write_response_body)
//     async fn write_response_body(
//         &mut self,
//         data: Option<Bytes>,
//         end_of_stream: bool,
//     ) -> Result<(), Box<Error>> {
//         self.deref_mut()
//             .write_response_body(data, end_of_stream)
//             .await
//     }
// }
//
// /// Type used to store remote user’s name in `SessionWrapper::extensions`
// #[derive(Debug, Clone)]
// struct RemoteUser(String);
//
// /// Type used to store original request URI in `SessionWrapper::extensions`
// #[derive(Debug, Clone)]
// struct OriginalUri(Uri);
//
// /// Creates a new Pingora session for tests with given request header
// pub async fn create_test_session(header: RequestHeader) -> Session {
//     create_test_session_with_body(header, "").await
// }
//
// /// Creates a new Pingora session for tests with given request header and request body
// pub async fn create_test_session_with_body(
//     mut header: RequestHeader,
//     body: impl AsRef<[u8]>,
// ) -> Session {
//     let mut cursor = Cursor::new(Vec::<u8>::new());
//     let _ = cursor.write(b"POST / HTTP/1.1\r\n");
//     let _ = cursor.write(b"Connection: close\r\n");
//     let _ = cursor.write(b"\r\n");
//     let _ = cursor.write(body.as_ref());
//     let _ = cursor.seek(SeekFrom::Start(0));
//
//     let _ = header.insert_header(header::CONTENT_LENGTH, body.as_ref().len());
//
//     let mut session = Session::new_h1(Box::new(cursor));
//     assert!(session.read_request().await.unwrap());
//     *session.req_header_mut() = header;
//
//     session
// }
//
// pub struct SessionWrapperImpl<'a> {
//     inner: &'a mut Session,
//     extensions: &'a mut Extensions,
//     capture_body: bool,
// }
//
// impl<'a> SessionWrapperImpl<'a> {
//     /// Creates a new session wrapper for the given Pingora session.
//     pub fn new(inner: &'a mut Session, extensions: &'a mut Extensions, capture_body: bool) -> Self {
//         Self {
//             inner,
//             extensions,
//             capture_body,
//         }
//     }
// }
//
// #[async_trait]
// impl SessionWrapper for SessionWrapperImpl<'_> {
//     fn extensions(&self) -> &Extensions {
//         self.extensions
//     }
//
//     fn extensions_mut(&mut self) -> &mut Extensions {
//         self.extensions
//     }
//
//     async fn write_response_body(
//         &mut self,
//         data: Option<Bytes>,
//         end_of_stream: bool,
//     ) -> Result<(), Box<Error>> {
//         if self.capture_body {
//             if let Some(data) = data {
//                 self.extensions_mut()
//                     .get_or_insert_default::<BytesMut>()
//                     .extend_from_slice(&data);
//             }
//             Ok(())
//         } else {
//             self.deref_mut()
//                 .write_response_body(data, end_of_stream)
//                 .await
//         }
//     }
// }
//
// impl Deref for SessionWrapperImpl<'_> {
//     type Target = Session;
//
//     fn deref(&self) -> &Self::Target {
//         self.inner
//     }
// }
//
// impl DerefMut for SessionWrapperImpl<'_> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         self.inner
//     }
// }
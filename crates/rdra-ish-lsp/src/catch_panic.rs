//! Keeps a panic in one handler from taking the whole server down.

use std::future::{poll_fn, Future};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::task::{Context, Poll};

use tower_lsp::jsonrpc::{Error, Request, Response};
use tower_service::Service;

/// Wraps the language service. tower-lsp polls every handler on the server's main task, so
/// a panic in one handler used to end the process. Here a request whose handler panics gets
/// an internal-error response, and a notification whose handler panics is dropped. The
/// default panic hook still prints the panic to stderr, which the editor logs.
pub struct CatchPanic<S>(pub S);

impl<S> Service<Request> for CatchPanic<S>
where
    S: Service<Request, Response = Option<Response>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Option<Response>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Response>, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let mut id = request.id().cloned();
        let mut handler = Box::pin(self.0.call(request));
        Box::pin(poll_fn(move |cx| {
            catch_unwind(AssertUnwindSafe(|| handler.as_mut().poll(cx))).unwrap_or_else(|_| {
                Poll::Ready(Ok(id
                    .take()
                    .map(|id| Response::from_error(id, Error::internal_error()))))
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::future::{ready, Ready};

    use tower_lsp::jsonrpc::Id;

    use super::*;

    struct Panics;

    impl Service<Request> for Panics {
        type Response = Option<Response>;
        type Error = ();
        type Future = Pin<Box<dyn Future<Output = Result<Option<Response>, ()>> + Send>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: Request) -> Self::Future {
            Box::pin(async { panic!("handler panicked") })
        }
    }

    struct Answers;

    impl Service<Request> for Answers {
        type Response = Option<Response>;
        type Error = ();
        type Future = Ready<Result<Option<Response>, ()>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: Request) -> Self::Future {
            ready(Ok(request
                .id()
                .cloned()
                .map(|id| Response::from_ok(id, serde_json::json!("ok")))))
        }
    }

    #[tokio::test]
    async fn a_panicking_request_gets_an_internal_error() {
        let request = Request::build("textDocument/hover").id(7).finish();
        let response = CatchPanic(Panics).call(request).await;
        assert_eq!(
            response,
            Ok(Some(Response::from_error(
                Id::Number(7),
                Error::internal_error()
            )))
        );
    }

    #[tokio::test]
    async fn a_panicking_notification_is_dropped() {
        let notification = Request::build("textDocument/didChange").finish();
        assert_eq!(CatchPanic(Panics).call(notification).await, Ok(None));
    }

    #[tokio::test]
    async fn a_handler_that_returns_is_passed_through() {
        let request = Request::build("textDocument/hover").id(8).finish();
        assert_eq!(
            CatchPanic(Answers).call(request).await,
            Ok(Some(Response::from_ok(
                Id::Number(8),
                serde_json::json!("ok")
            )))
        );
    }
}

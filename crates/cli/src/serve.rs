use std::convert::Infallible;
use std::fmt::Display;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_lite::{future, stream, Stream, StreamExt};
use http_body_util::{Either, StreamBody};
use hyper::body::{Bytes, Frame};
use hyper::header::{self, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::runtime::Builder;
use tokio::sync::broadcast;
use tower_async::{Service, ServiceBuilder};
use tower_async_http::compression::CompressionLayer;
use tower_async_http::services::ServeDir;

use crate::build::build;
use crate::log;
use crate::report::{ErrorExt, Report};
use crate::Serve;

pub fn serve(s: &Serve) -> Report<()> {
    build(&s.build)?;

    Builder::new_current_thread()
        .enable_all()
        .build()
        .message("failed to create tokio runtime")?
        .block_on(start(s))
}

async fn start(s: &Serve) -> Report<()> {
    let listener = TcpListener::bind((s.address.as_str(), s.port))
        .await
        .with_message(|| format!("failed to bind tcp listener to {}:{}", s.address, s.port))?;

    log::starting!("development server at http://{}:{}", s.address, s.port);

    let (events, _) = broadcast::channel(1);

    tokio::spawn({
        let events = events.clone();
        async move {
            loop {
                _ = events.send(Update);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    });

    let serve = {
        let files = ServiceBuilder::new()
            .layer(CompressionLayer::new())
            .service(ServeDir::new(&s.build.dist));

        let serve = ServiceBuilder::new().layer_fn(Logger).service(Events {
            path: "/events",
            events,
            inner: files,
        });

        Arc::new(serve)
    };

    loop {
        let (tcp, _) = listener
            .accept()
            .await
            .message("failed to accept tcp connection")?;

        let serve = serve.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(tcp);
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(|req| serve.call(req)))
                .await
            {
                if !err.is_incomplete_message() {
                    log::error!("serving connection: {err}");
                }
            }
        });
    }
}

#[derive(Clone, Copy)]
struct Update;

type EventStream = StreamBody<Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, Infallible>> + Send>>>;

struct Events<S> {
    path: &'static str,
    events: broadcast::Sender<Update>,
    inner: S,
}

impl<S> Events<S> {
    fn event_stream(&self) -> EventStream {
        let receiver = self.events.subscribe();
        let stream = stream::unfold(receiver, |mut receiver| async {
            let recv = async {
                receiver.recv().await.ok()?;
                const { Some(Bytes::from_static(b"event: update\ndata: {}\n\n")) }
            };

            let ping = async {
                tokio::time::sleep(Duration::from_secs(15)).await;
                const { Some(Bytes::from_static(b":\n\n")) }
            };

            let chunk = future::or(recv, ping).await?;
            Some((chunk, receiver))
        })
        .map(Frame::data)
        .map(Ok)
        .boxed();

        StreamBody::new(stream)
    }
}

impl<S, I, O> Service<Request<I>> for Events<S>
where
    S: Service<Request<I>, Response = Response<O>>,
{
    type Response = Response<Either<EventStream, O>>;
    type Error = S::Error;

    async fn call(&self, req: Request<I>) -> Result<Self::Response, Self::Error> {
        let path = req.uri().path();
        let path = path.strip_suffix('/').unwrap_or(path);

        if path == self.path {
            let mut res = Response::new(Either::Left(self.event_stream()));
            let headers = res.headers_mut();

            headers.insert(
                header::CONTENT_TYPE,
                const { HeaderValue::from_static("text/event-stream") },
            );

            headers.insert(
                header::CACHE_CONTROL,
                const { HeaderValue::from_static("no-cache") },
            );

            Ok(res)
        } else {
            let res = self.inner.call(req).await?;
            Ok(res.map(Either::Right))
        }
    }
}

struct Logger<S>(S);

impl<S, I, O> Service<Request<I>> for Logger<S>
where
    S: Service<Request<I>, Response = Response<O>, Error: Display>,
{
    type Response = S::Response;
    type Error = S::Error;

    async fn call(&self, req: Request<I>) -> Result<Self::Response, Self::Error> {
        let method = req.method().to_owned();
        let uri = req.uri().to_owned();

        let out = self.0.call(req).await;

        match &out {
            Ok(res) => log::info!("{method} {uri} -> {}", res.status()),
            Err(err) => log::error!("{method} {uri} -> {err}"),
        }

        out
    }
}

use std::{fmt::Display, sync::Arc};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::runtime::Builder;
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

    let serve = {
        let serve = ServiceBuilder::new()
            .layer_fn(Logger)
            .layer(CompressionLayer::new())
            .service(ServeDir::new(&s.build.dist));

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
                log::error!("serving connection: {err}");
            }
        });
    }
}

pub struct Logger<S>(S);

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

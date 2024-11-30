use std::net::Ipv4Addr;
use std::path::Path;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::runtime::Builder;
use tower_async::Service;
use tower_async_http::services::ServeDir;

use crate::build::build;
use crate::log;
use crate::report::{ErrorExt, Report};
use crate::Build;

pub fn serve(b: &Build) -> Report<()> {
    build(b)?;

    Builder::new_current_thread()
        .enable_all()
        .build()
        .message("failed to create tokio runtime")?
        .block_on(start(&b.dist))
}

async fn start(dist: &Path) -> Report<()> {
    let ip = Ipv4Addr::LOCALHOST;
    let port = 3000;

    let listener = TcpListener::bind((ip, port))
        .await
        .with_message(|| format!("failed to bind tcp listener to {ip}:{port}"))?;

    log::starting!("development server at http://{ip}:{port}");

    let serve_dist: &_ = Box::leak(Box::new(ServeDir::new(dist)));

    loop {
        let (tcp, _) = listener
            .accept()
            .await
            .message("failed to accept tcp connection")?;

        tokio::spawn(async {
            let io = TokioIo::new(tcp);
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(|req| serve_dist.call(req)))
                .await
            {
                log::error!("serving connection: {err}");
            }
        });
    }
}

use std::convert::Infallible;
use std::net::Ipv4Addr;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::runtime::Builder;

use crate::log;
use crate::report::{Report, ReportExt};

pub fn serve() -> Result<(), Report> {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .with_message("failed to create tokio runtime")?
        .block_on(start())
}

async fn start() -> Result<(), Report> {
    let ip = Ipv4Addr::LOCALHOST;
    let port = 3000;

    let listener = TcpListener::bind((ip, port))
        .await
        .with_message(format!("failed to bind tcp listener to {ip}:{port}"))?;

    log::starting!("development server at http://{ip}:{port}");

    loop {
        let (tcp, _) = listener
            .accept()
            .await
            .with_message("failed to accept tcp connection")?;

        async fn hello(_: Request<Incoming>) -> Result<Response<String>, Infallible> {
            Ok(Response::new(String::from("Hello World!")))
        }

        tokio::spawn(async {
            let io = TokioIo::new(tcp);
            if let Err(err) = http1::Builder::new()
                .timer(TokioTimer::new())
                .serve_connection(io, service_fn(hello))
                .await
            {
                log::error!("serving connection: {err}");
            }
        });
    }
}

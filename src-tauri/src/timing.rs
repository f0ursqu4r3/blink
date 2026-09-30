//! Request phase timing: DNS through a resolver, connection setup through a
//! connector layer, and the rest from timestamps around the send.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde::Serialize;
use std::time::Instant;

/// Time spent in each phase of one request, summed over redirect hops.
#[derive(Default)]
pub struct Phases {
    dns: Mutex<Duration>,
    /// Connection setup, including DNS, TCP, TLS and a proxy tunnel.
    connect: Mutex<Duration>,
}

impl Phases {
    fn add(slot: &Mutex<Duration>, elapsed: Duration) {
        *slot.lock().unwrap() += elapsed;
    }

    /// Split `until_headers` and `download` into phases, in milliseconds.
    pub fn report(&self, until_headers: Duration, download: Duration) -> Timing {
        let dns = *self.dns.lock().unwrap();
        let setup = *self.connect.lock().unwrap();
        let connect = setup.saturating_sub(dns);
        let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
        Timing {
            dns_ms: ms(dns),
            connect_ms: ms(connect),
            wait_ms: ms(until_headers.saturating_sub(setup)),
            download_ms: ms(download),
        }
    }
}

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    pub dns_ms: f64,
    /// TCP and TLS setup. The connector does not report them apart.
    pub connect_ms: f64,
    /// From the connection until the response headers: send and server time.
    pub wait_ms: f64,
    pub download_ms: f64,
}

/// The system resolver, timed.
pub struct TimedResolver(pub Arc<Phases>);

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let phases = self.0.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            let started = Instant::now();
            let result = tokio::net::lookup_host((host.as_str(), 0)).await;
            Phases::add(&phases.dns, started.elapsed());
            let addrs: Vec<SocketAddr> = result?.collect();
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// A connector layer that times connection setup.
#[derive(Clone)]
pub struct TimedConnectLayer(pub Arc<Phases>);

impl<S> tower_layer::Layer<S> for TimedConnectLayer {
    type Service = TimedConnect<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimedConnect {
            inner,
            phases: self.0.clone(),
        }
    }
}

#[derive(Clone)]
pub struct TimedConnect<S> {
    inner: S,
    phases: Arc<Phases>,
}

impl<S, R> tower_service::Service<R> for TimedConnect<S>
where
    S: tower_service::Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let phases = self.phases.clone();
        let started = Instant::now();
        let future = self.inner.call(request);
        Box::pin(async move {
            let result = future.await;
            Phases::add(&phases.connect, started.elapsed());
            result
        })
    }
}

//! Per-I/O inactivity limits for the pinned ureq 3.4 transport API. A body
//! deadline would reject healthy multi-gigabyte models on slow connections.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};

#[derive(Debug)]
struct InactivityConnector(Duration, Arc<AtomicBool>);

impl<T: Transport> Connector<T> for InactivityConnector {
    type Out = InactivityTransport<T>;

    fn connect(
        &self,
        _: &ConnectionDetails,
        transport: Option<T>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(transport.map(|inner| InactivityTransport {
            inner,
            limit: self.0,
            body_started: self.1.clone(),
        }))
    }
}

#[derive(Debug)]
struct InactivityTransport<T> {
    inner: T,
    limit: Duration,
    body_started: Arc<AtomicBool>,
}

impl<T: Transport> InactivityTransport<T> {
    fn bounded(&self, mut timeout: NextTimeout) -> NextTimeout {
        // ureq 3.4 carries RecvResponse into body reads as a predecessor
        // deadline. Once call() returned headers, only inactivity is relevant.
        if self.body_started.load(Ordering::Relaxed) || timeout.after > self.limit.into() {
            timeout.after = self.limit.into();
            timeout.reason = ureq::Timeout::RecvBody;
        }
        timeout
    }
}

impl<T: Transport> Transport for InactivityTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, self.bounded(timeout))
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.inner.await_input(self.bounded(timeout))
    }
    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

pub(super) fn agent_with_inactivity_timeout(limit: Duration) -> (ureq::Agent, Arc<AtomicBool>) {
    let body_started = Arc::new(AtomicBool::new(false));
    let config = ureq::Agent::config_builder()
        .timeout_resolve(Some(limit))
        .timeout_connect(Some(limit))
        .timeout_send_request(Some(limit))
        .timeout_recv_response(Some(limit))
        .build();
    let agent = ureq::Agent::with_parts(
        config,
        DefaultConnector::default().chain(InactivityConnector(limit, body_started.clone())),
        ureq::unversioned::resolver::DefaultResolver::default(),
    );
    (agent, body_started)
}

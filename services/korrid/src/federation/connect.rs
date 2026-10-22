//! Race transport reachability, never HTTP requests or peer mutations.
use futures::{stream::FuturesUnordered, StreamExt};
use std::{future::Future, io, time::Duration};
use tokio::time::{timeout, Instant};

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const CONNECT_STAGGER: Duration = Duration::from_millis(250);

/// The caller retains its aggregate deadline. Probes send no application bytes;
/// reqwest subsequently sends to only the selected origin (and reconnects).
pub(crate) async fn select_origin(origins: &[String]) -> io::Result<usize> {
    race_with(origins, connect).await
}

async fn connect(origin: String) -> io::Result<()> {
    let url = url::Url::parse(&origin).map_err(io::Error::other)?;
    let host = match url
        .host()
        .ok_or_else(|| io::Error::other("missing peer host"))?
    {
        url::Host::Domain(host) => host.to_owned(),
        url::Host::Ipv4(ip) => ip.to_string(),
        url::Host::Ipv6(ip) => ip.to_string(),
    };
    let port = url
        .port_or_known_default()
        .ok_or_else(|| io::Error::other("missing peer port"))?;
    let _stream = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
    Ok(())
}

async fn race_with<F, D>(origins: &[String], dial: F) -> io::Result<usize>
where
    F: Fn(String) -> D,
    D: Future<Output = io::Result<()>>,
{
    let start = |index: usize| {
        let dial = dial(origins[index].clone());
        async move {
            match timeout(CONNECT_TIMEOUT, dial).await {
                Ok(result) => result.map(|()| index),
                Err(_) => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "peer connect timed out",
                )),
            }
        }
    };
    let mut pending = FuturesUnordered::new();
    let mut next = 0;
    let mut launch_at = Instant::now();
    let mut last_error = io::Error::new(io::ErrorKind::NotConnected, "no eligible peer endpoint");
    loop {
        if pending.is_empty() {
            if next == origins.len() {
                return Err(last_error);
            }
            pending.push(start(next));
            next += 1;
            launch_at = Instant::now() + CONNECT_STAGGER;
        }
        tokio::select! {
            biased;
            Some(result) = pending.next() => match result {
                Ok(index) => return Ok(index), // Drops all losing probes before any HTTP send.
                Err(error) => last_error = error,
            },
            _ = tokio::time::sleep_until(launch_at), if next < origins.len() => {
                pending.push(start(next));
                next += 1;
                launch_at = Instant::now() + CONNECT_STAGGER;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn a_400ms_connect_is_usable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origins = vec![format!("http://{}", listener.local_addr().unwrap())];
        let start = Instant::now();
        let selected = race_with(&origins, |origin| async move {
            // Controlled connect latency around a real TCP dial, without an
            // external slow network, privileged netem, or a replacement client.
            tokio::time::sleep(Duration::from_millis(400)).await;
            connect(origin).await
        })
        .await
        .unwrap();
        assert_eq!(selected, 0);
        assert!(start.elapsed() >= Duration::from_millis(400));
        assert!(start.elapsed() < CONNECT_TIMEOUT);
    }
    #[tokio::test]
    async fn unreachable_first_candidate_does_not_delay_the_next_until_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let good = format!("http://{}", listener.local_addr().unwrap());
        let origins = vec!["http://unreachable".into(), good];
        let start = Instant::now();
        let selected = race_with(&origins, |origin| async move {
            if origin == "http://unreachable" {
                std::future::pending::<()>().await;
            }
            connect(origin).await
        })
        .await
        .unwrap();
        assert_eq!(selected, 1);
        assert!(start.elapsed() >= CONNECT_STAGGER);
        assert!(start.elapsed() < Duration::from_millis(750));
    }
    #[tokio::test]
    async fn eight_pending_connects_fit_within_the_prelaunch_deadline() {
        let origins = vec!["http://unreachable".into(); 8];
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            race_with(&origins, |_| async {
                std::future::pending::<io::Result<()>>().await
            }),
        )
        .await;
        assert!(matches!(result, Ok(Err(_))));
    }
}

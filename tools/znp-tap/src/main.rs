//! Sits between Zigbee2MQTT and the coordinator, forwards bytes unchanged and
//! records every ZNP frame (direction, time, raw hex) to a log file.
//!
//! Example: `znp-tap --listen 0.0.0.0:6638 --upstream 192.168.1.50:6638 --out capture.log`
//! and point Z2M at `tcp://<this-host>:6638`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::Parser;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_util::codec::Decoder;
use znp::ZnpCodec;

#[derive(Parser)]
struct Args {
    /// Address Zigbee2MQTT connects to.
    #[arg(long, default_value = "0.0.0.0:6638")]
    listen: String,
    /// Coordinator address, e.g. the SLZB-06.
    #[arg(long)]
    upstream: String,
    /// Capture file, appended to.
    #[arg(long, default_value = "capture.log")]
    out: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let out = File::options()
        .create(true)
        .append(true)
        .open(&args.out)
        .await
        .context("opening capture file")?;
    let out = Arc::new(Mutex::new(out));
    let listener = TcpListener::bind(&args.listen).await?;
    tracing::info!(listen = %args.listen, upstream = %args.upstream, "waiting for host");

    loop {
        let (host, peer) = listener.accept().await?;
        tracing::info!(%peer, "host connected");
        let coordinator = TcpStream::connect(&args.upstream)
            .await
            .context("connecting to coordinator")?;
        let (host_rx, host_tx) = host.into_split();
        let (coord_rx, coord_tx) = coordinator.into_split();
        let to_coord = pump(host_rx, coord_tx, "host->coord", out.clone());
        let to_host = pump(coord_rx, host_tx, "coord->host", out.clone());
        if let Err(err) = tokio::try_join!(to_coord, to_host) {
            tracing::warn!(%err, "session ended");
        }
    }
}

async fn pump(
    mut rx: impl AsyncReadExt + Unpin,
    mut tx: impl AsyncWriteExt + Unpin,
    direction: &'static str,
    out: Arc<Mutex<File>>,
) -> anyhow::Result<()> {
    let mut codec = ZnpCodec;
    let mut pending = bytes::BytesMut::new();
    let mut buf = [0u8; 512];
    loop {
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            anyhow::bail!("{direction}: connection closed");
        }
        tx.write_all(&buf[..n]).await?;
        pending.extend_from_slice(&buf[..n]);
        while let Some(frame) = codec.decode(&mut pending)? {
            let millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
            let data: String = frame.data.iter().map(|b| format!("{b:02x}")).collect();
            let line = format!(
                "{millis} {direction} {:?} {:?} 0x{:02x} {data}\n",
                frame.frame_type, frame.subsystem, frame.command
            );
            tracing::debug!("{}", line.trim_end());
            out.lock().await.write_all(line.as_bytes()).await?;
        }
    }
}

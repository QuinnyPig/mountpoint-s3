use std::io::Write;
use std::sync::{Arc, Mutex};

use aws_crt_s3::common::rust_log_adapter::RustLogAdapter;
use clap::{Arg, Command};
use futures::StreamExt;
use s3_client::{ObjectClient, S3Client};
use tracing_subscriber::fmt::Subscriber;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Like `tracing_subscriber::fmt::init` but sends logs to stderr
fn init_tracing_subscriber() {
    RustLogAdapter::try_init().expect("unable to install CRT log adapter");

    let subscriber = Subscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .finish();

    subscriber.try_init().expect("unable to install global subscriber");
}

fn main() {
    init_tracing_subscriber();

    let matches = Command::new("download")
        .about("Download a single key from S3")
        .arg(Arg::new("bucket").required(true))
        .arg(Arg::new("key").required(true))
        .arg(Arg::new("region").long("region").default_value("us-east-1"))
        .get_matches();

    let bucket = matches.get_one::<String>("bucket").unwrap();
    let key = matches.get_one::<String>("key").unwrap();
    let region = matches.get_one::<String>("region").unwrap();

    let client = S3Client::new(region, Default::default()).expect("couldn't create client");

    let last_offset = Arc::new(Mutex::new(None));
    let last_offset_clone = Arc::clone(&last_offset);
    futures::executor::block_on(async move {
        let mut request = client
            .get_object(bucket, key, None)
            .await
            .expect("couldn't create get request");
        loop {
            match StreamExt::next(&mut request).await {
                Some(Ok((offset, body))) => {
                    let mut last_offset = last_offset_clone.lock().unwrap();
                    assert!(Some(offset) > *last_offset, "out-of-order body parts");
                    *last_offset = Some(offset);
                    let stdout = std::io::stdout();
                    let mut guard = stdout.lock();
                    guard.write_all(&body[..]).expect("write failed");
                }
                Some(Err(e)) => {
                    tracing::error!(error = ?e, "request failed");
                    break;
                }
                None => break,
            }
        }
    });
}
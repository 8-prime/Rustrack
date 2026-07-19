//! Break down where time goes when a LIF layout is uploaded.
//!
//! Usage: cargo run --release --example lif_timing -- <path-to.lif>

use std::io::Write;
use std::time::Instant;

use flate2::{Compression, write::GzEncoder};
use shared::lif::{Lif, LifSummary, validate};

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("usage: lif_timing <file.lif>");
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };

    let t = Instant::now();
    let raw = std::fs::read(&path)?;
    let read_ms = t.elapsed().as_secs_f64() * 1000.0;
    let mb = raw.len() as f64 / 1024.0 / 1024.0;

    println!("profile: {profile}");
    println!("file:    {:.1} MB\n", mb);

    let t = Instant::now();
    let lif: Lif = serde_json::from_slice(&raw)?;
    let parse_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    validate(&lif).map_err(|e| anyhow::anyhow!("{e}"))?;
    let validate_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let summary = LifSummary::derive(&lif, raw.len() as u64, "now".into());
    let summarize_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    drop(lif);
    let drop_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw)?;
    let gz = enc.finish()?;
    let gzip_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(&raw)?;
    let gz_fast = enc.finish()?;
    let gzip_fast_ms = t.elapsed().as_secs_f64() * 1000.0;

    let total = parse_ms + validate_ms + summarize_ms + drop_ms + gzip_ms;

    println!("read file      {read_ms:8.0} ms");
    println!("parse          {parse_ms:8.0} ms   ({:.0} MB/s)", mb / (parse_ms / 1000.0));
    println!("validate       {validate_ms:8.0} ms");
    println!("summarize      {summarize_ms:8.0} ms");
    println!("drop parsed    {drop_ms:8.0} ms");
    println!(
        "gzip default   {gzip_ms:8.0} ms   ({:.0} MB/s, -> {:.2} MB, {:.1}%)",
        mb / (gzip_ms / 1000.0),
        gz.len() as f64 / 1024.0 / 1024.0,
        gz.len() as f64 / raw.len() as f64 * 100.0
    );
    println!(
        "gzip fast      {gzip_fast_ms:8.0} ms   ({:.0} MB/s, -> {:.2} MB, {:.1}%)",
        mb / (gzip_fast_ms / 1000.0),
        gz_fast.len() as f64 / 1024.0 / 1024.0,
        gz_fast.len() as f64 / raw.len() as f64 * 100.0
    );
    println!("---");
    println!("blocking total {total:8.0} ms   (parse+validate+gzip default)");
    println!("  nodes {} edges {}", summary.node_count, summary.edge_count);

    Ok(())
}

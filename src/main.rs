use prometheus_exporter_base::{render_prometheus, MetricType, PrometheusMetric};
use std::thread;
use std::time::Duration;
use std::process::Command;
use regex::Regex;
use std::path::PathBuf;
use std::fs;

use std::sync::{Arc, Mutex};

use hashbrown::HashMap;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use serde_derive::Deserialize;

#[derive(Debug, Clone, Default)]
struct Targets {}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "fping_exporter")]
struct Opt {
    /// Address to listen on for web interface and telemetry.
    #[structopt(long = "listen-address", default_value = "0.0.0.0:9115")]
    web_listen_addr: String,

    #[structopt(long = "targets")]
    targets: Vec<String>,
}

fn main() {
    // get targets from either config file or command line
    let config = fs::read_to_string("fping.toml").unwrap();
    let options = Opt::from_args_with_toml(&config).expect("toml parse failed");

    let web_addr = options.web_listen_addr.parse().expect("can not parse listen addr");
    println!("starting exporter on {}", web_addr);

    render_prometheus(web_addr, options, |request, options| {
        async move {
            println!("{:?} {:?}", request.headers(), request.uri());

            let mut command_results = vec![];
            for target_subnet in options.targets.clone() {
                let output = Command::new("/usr/local/sbin/fping")
                    .args(&["-q", "-r", "0", "-c", "5", "-g", &target_subnet])
                    .output()
                    .expect("failed to execute process");

                let stderr = output.clone().stderr;
                let output = String::from_utf8_lossy(&stderr);

                for line in output.lines() {
                    command_results.push(line.to_owned());
                }
            }

            let mut output_string = String::new();
            let mut measurements = HashMap::new();
            let mut packet_stats = HashMap::new();
            let mut packet_losses = HashMap::new();

            // Parse the fping output (tested on version 4.2)
            // example ping "8.8.8.8 : xmt/rcv/%loss = 10/10/0%, min/avg/max = 0.72/0.82/1.42"
            // example loss "192.1.1.1 : xmt/rcv/%loss = 10/0/100%"
            // https://www.debuggex.com/r/T5_Da8_kWGHpm8y1
            let re = Regex::new(r"(?P<ip_address>.*) :.*= (?P<sent>\d+)/(?P<received>\d+)/(?P<lost>\d+)%(?:,.*= (?P<min>\d+\.?\d*)/(?P<avg>\d+\.?\d*)/(?P<max>\d+\.?\d*))?").unwrap();

            for ping_result in command_results {
                let caps = re.captures(&ping_result).unwrap();

                let ip_address = caps.name("ip_address").unwrap().as_str();

                let sent: u8 = caps.name("sent").unwrap().as_str().parse().unwrap();
                let received: u8 = caps.name("received").unwrap().as_str().parse().unwrap();
                let lost: u8 = caps.name("lost").unwrap().as_str().parse().unwrap();

                packet_stats.insert(ip_address.to_owned(), (sent, received));
                packet_losses.insert(ip_address.to_owned(), lost);

                if caps.name("min").is_some() {
                    let min: f32 = caps.name("min").unwrap().as_str().parse().unwrap();
                    let avg: f32 = caps.name("avg").unwrap().as_str().parse().unwrap();
                    let max: f32 = caps.name("max").unwrap().as_str().parse().unwrap();

                    measurements.insert(ip_address.to_owned(), (min / 1000f32, avg / 1000f32, max / 1000f32));
                }
            }

            // make output
            // measurements (min, avg max)
            let ping_rtt = PrometheusMetric::new("ping_rtt_seconds", MetricType::Gauge, "Ping round trip time in seconds");
            output_string.push_str(&ping_rtt.render_header());
            for (ip_address, (min, avg, max)) in &measurements {
                let mut attributes = Vec::new();
                attributes.push(("address", &ip_address[..]));
                attributes.push(("sample", "minimum"));
                output_string.push_str(&ping_rtt.render_sample(Some(&attributes), *min));

                let mut attributes = Vec::new();
                attributes.push(("address", &ip_address[..]));
                attributes.push(("sample", "average"));
                output_string.push_str(&ping_rtt.render_sample(Some(&attributes), *avg));

                let mut attributes = Vec::new();
                attributes.push(("address", &ip_address[..]));
                attributes.push(("sample", "maximum"));
                output_string.push_str(&ping_rtt.render_sample(Some(&attributes), *max));
            }

            output_string.push_str("\n\n");

            let ping_packets_sent = PrometheusMetric::new("ping_packets_sent", MetricType::Gauge, "Ping packets sent");
            output_string.push_str(&ping_packets_sent.render_header());

            // packets sent/received
            for (ip_address, (sent, _receieved)) in &packet_stats {
                let mut attributes = Vec::new();
                attributes.push(("address", &ip_address[..]));
                output_string.push_str(&ping_packets_sent.render_sample(Some(&attributes), *sent));
            }

            output_string.push_str("\n\n");
            let ping_packets_received = PrometheusMetric::new("ping_packets_received", MetricType::Gauge, "Ping packets receieved");
            output_string.push_str(&ping_packets_received.render_header());

            for (ip_address, (_sent, receieved)) in &packet_stats {
                let mut attributes = Vec::new();
                attributes.push(("address", &ip_address[..]));
                output_string.push_str(&ping_packets_received.render_sample(Some(&attributes), *receieved));
            }

            output_string.push_str("\n\n");

            // packets lost as a percentage
            let ping_packet_loss = PrometheusMetric::new("ping_packet_loss_percent", MetricType::Gauge, "Percent of ping packets lost");
            output_string.push_str(&ping_packet_loss.render_header());

            for (ip_address, lost) in &packet_losses {
                let mut attributes = Vec::new();
                attributes.push(("address", &ip_address[..]));
                output_string.push_str(&ping_packet_loss.render_sample(Some(&attributes), *lost));
            }

            Ok(output_string)
        }
    });
}

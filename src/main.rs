use prometheus_exporter_base::{render_prometheus, MetricType, PrometheusMetric};
use regex::Regex;
use std::process::Command;

use hashbrown::HashMap;
use structopt::StructOpt;

use ipnet::IpNet;
use std::net::IpAddr;

use snafu::{OptionExt, ResultExt, Snafu};
use percent_encoding::percent_decode_str;

#[derive(StructOpt, Debug)]
#[structopt(name = "fping_exporter")]
struct Opt {
    /// Address to listen on for web interface and telemetry.
    #[structopt(long = "listen-address", default_value = "0.0.0.0:9215")]
    web_listen_addr: String,

    // #[structopt(long = "targets")]
    // targets: Vec<IpNet>,
}

#[derive(Debug)]
struct PingResult {
    ip_address: IpAddr,
    sent: u8,
    received: u8,
    lost: u8,
    minimum: Option<f64>,
    average: Option<f64>,
    maxiumum: Option<f64>,
}

#[derive(Debug, Snafu)]
enum ExporterError {
    #[snafu(display("Unable to parse fping output"))]
    ParseError,

    #[snafu(display("Unable to find `{}` field in fping output", name))]
    MissingField { name: String },

    #[snafu(display("Error running fping"))]
    CommandError { source: std::io::Error },

    #[snafu(display("Error parsing IP Address: {}", ip_address_output))]
    IpAddressError {
        ip_address_output: String,
        source: std::net::AddrParseError,
    },

    #[snafu(display("Unable to parse fping output"))]
    ParseIntError { source: std::num::ParseIntError },

    #[snafu(display("Unable to parse fping output"))]
    ParseFloatError { source: std::num::ParseFloatError },
}

type Result<T, E = ExporterError> = std::result::Result<T, E>;

fn process_subnet(target_subnet: IpNet) -> Result<Vec<PingResult>> {
    let subnet_string = format!("{:?}", target_subnet);
    let output = Command::new("/usr/local/sbin/fping")
        .args(&["-q", "-r", "0", "-c", "5", "-g", &subnet_string])
        .output()
        .context(CommandError)?;

    let stderr = output.clone().stderr;
    let output = String::from_utf8_lossy(&stderr);

    let mut results = vec![];

    let re = Regex::new(r"(?P<ip_address>.*) :.*= (?P<sent>\d+)/(?P<received>\d+)/(?P<lost>\d+)%(?:,.*= (?P<min>\d+\.?\d*)/(?P<avg>\d+\.?\d*)/(?P<max>\d+\.?\d*))?").unwrap();
    for ping_result in output.lines() {
        let caps = re.captures(&ping_result).ok_or(ExporterError::ParseError)?;

        let ip_address_output = caps
            .name("ip_address")
            .context(MissingField {
                name: "ip_address".to_string(),
            })?
            .as_str()
            .trim();

        let ip_address: IpAddr = ip_address_output
            .parse()
            .context(IpAddressError { ip_address_output })?;

        let sent_output = caps
            .name("sent")
            .context(MissingField {
                name: "sent".to_string(),
            })?
            .as_str();
        let sent: u8 = sent_output.parse().context(ParseIntError)?;

        let received_output = caps
            .name("received")
            .context(MissingField {
                name: "received".to_string(),
            })?
            .as_str();
        let received: u8 = received_output.parse().context(ParseIntError)?;

        let lost_output = caps
            .name("lost")
            .context(MissingField {
                name: "lost".to_string(),
            })?
            .as_str();
        let lost: u8 = lost_output.parse().context(ParseIntError)?;

        let mut minimum = None;
        let mut average = None;
        let mut maxiumum = None;

        if caps.name("min").is_some() {
            let min_ms: f64 = caps
                .name("min")
                .context(MissingField {
                    name: "min".to_string(),
                })?
                .as_str()
                .parse()
                .context(ParseFloatError)?;
            let avg_ms: f64 = caps
                .name("avg")
                .context(MissingField {
                    name: "avg".to_string(),
                })?
                .as_str()
                .parse()
                .context(ParseFloatError)?;
            let max_ms: f64 = caps
                .name("max")
                .context(MissingField {
                    name: "max".to_string(),
                })?
                .as_str()
                .parse()
                .context(ParseFloatError)?;

            minimum = Some(min_ms / 1000.0);
            average = Some(avg_ms / 1000.0);
            maxiumum = Some(max_ms / 1000.0);
        }

        results.push(PingResult {
            ip_address,
            sent,
            received,
            lost,
            minimum,
            average,
            maxiumum,
        })
    }

    Ok(results)
}

fn main() -> Result<()> {
    let options = Opt::from_args();

    let web_addr = options
        .web_listen_addr
        .parse()
        .expect("can not parse listen addr");

    println!("starting exporter on {}", web_addr);

    render_prometheus(web_addr, options, |request, _options| {
        async move {
            println!("{:?}", request.uri());

            let mut query_string = HashMap::new();
            if request.uri().query().is_some() {
                let query_decoded = percent_decode_str(request.uri().query().unwrap()).decode_utf8_lossy();
                let pairs = query_decoded.split("&");
                for p in pairs {
                    let mut sp = p.splitn(2, '=');
                    let (k, v) = (sp.next().unwrap(), sp.next().unwrap());
                    query_string.insert(k.to_owned(), v.to_owned());
                }
            }

            println!("{:?}", query_string);

            let target: IpNet = query_string.get("target").unwrap().parse().unwrap();
            let subnet_results = process_subnet(target)?;

            // make output
            let mut output_string = String::new();
            // measurements (min, avg max)
            let ping_rtt = PrometheusMetric::new(
                "ping_rtt_seconds",
                MetricType::Gauge,
                "Ping round trip time in seconds",
            );

            output_string.push_str(&ping_rtt.render_header());

            for result in &subnet_results {
                if result.minimum.is_none() {
                    continue;
                }
                let ip = result.ip_address.to_owned().to_string();

                let mut attributes = Vec::new();
                attributes.push(("address", &ip[..]));
                attributes.push(("sample", "minimum"));
                output_string.push_str(&ping_rtt.render_sample(Some(&attributes), result.minimum.unwrap()));

                attributes = Vec::new();
                attributes.push(("address", &ip[..]));
                attributes.push(("sample", "average"));
                output_string.push_str(&ping_rtt.render_sample(Some(&attributes), result.average.unwrap()));

                attributes = Vec::new();
                attributes.push(("address", &ip[..]));
                attributes.push(("sample", "maxiumum"));
                output_string.push_str(&ping_rtt.render_sample(Some(&attributes), result.maxiumum.unwrap()));
            }

            output_string.push_str("\n\n");

            // packets sent/received
            let ping_packets_sent = PrometheusMetric::new("ping_packets_sent", MetricType::Gauge, "Ping packets sent");
            output_string.push_str(&ping_packets_sent.render_header());

            for result in &subnet_results {
                let ip = result.ip_address.to_owned().to_string();
                let mut attributes = Vec::new();
                attributes.push(("address", &ip[..]));
                output_string.push_str(&ping_packets_sent.render_sample(Some(&attributes), result.sent));
            }

            output_string.push_str("\n\n");

            let ping_packets_received = PrometheusMetric::new("ping_packets_received", MetricType::Gauge, "Ping packets received");
            output_string.push_str(&ping_packets_received.render_header());

            for result in &subnet_results {
                let ip = result.ip_address.to_owned().to_string();
                let mut attributes = Vec::new();
                attributes.push(("address", &ip[..]));
                output_string.push_str(&ping_packets_received.render_sample(Some(&attributes), result.received));
            }

            output_string.push_str("\n\n");

            // packets lost as a percentage
            let ping_packet_loss = PrometheusMetric::new("ping_packet_loss_percent", MetricType::Gauge, "Percent of ping packets lost");
            output_string.push_str(&ping_packet_loss.render_header());

            for result in &subnet_results {
                let ip = result.ip_address.to_owned().to_string();
                let mut attributes = Vec::new();
                attributes.push(("address", &ip[..]));
                output_string.push_str(&ping_packet_loss.render_sample(Some(&attributes), result.lost));
            }

            Ok(output_string)
        }
    });

    Ok(())
}

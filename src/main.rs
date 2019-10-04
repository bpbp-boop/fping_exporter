mod ping_result;

use prometheus_exporter_base::{render_prometheus, MetricType, PrometheusMetric};
use std::process::Command;

use hashbrown::HashMap;
use structopt::StructOpt;

use ipnet::IpNet;

use snafu::{ResultExt, Snafu};
use percent_encoding::percent_decode_str;

use ping_result::{PingResult, FpingParseError};

#[derive(StructOpt, Debug)]
#[structopt(name = "fping_exporter")]
struct Opt {
    /// Address to listen on for web interface and telemetry.
    #[structopt(long = "listen-address", default_value = "0.0.0.0:9215")]
    web_listen_addr: String,

    // #[structopt(long = "targets")]
    // targets: Vec<IpNet>,
}

#[derive(Debug, Snafu)]
enum ExporterError {
     #[snafu(display("Error running fping"))]
    RunCommand { source: std::io::Error },

    #[snafu(display("Error running fping"))]
    Parse { source: FpingParseError },
}

type Result<T, E = ExporterError> = std::result::Result<T, E>;

fn process_subnet(target_subnet: IpNet) -> Result<Vec<PingResult>> {
    let subnet_string = format!("{:?}", target_subnet);
    let output = Command::new("/usr/local/sbin/fping")
        .args(&["-q", "-r", "0", "-c", "5", "-g", &subnet_string])
        .output()
        .context(RunCommand)?;

    let stderr = output.clone().stderr;
    let output = String::from_utf8_lossy(&stderr);

    let mut results = vec![];

    for result in output.lines() {
        let ping_result: PingResult = result.parse().context(Parse)?;
        results.push(ping_result)
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

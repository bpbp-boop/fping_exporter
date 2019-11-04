mod ping_result;

#[macro_use]
extern crate log;
extern crate simple_logger;

use actix_web::{middleware, web, App, HttpResponse, HttpServer, Responder};

use prometheus_exporter_base::{MetricType, PrometheusMetric};
use std::fs;
use std::process::Command;
use std::sync::{Arc};
use std::thread;
use std::time::{Duration, Instant};
use std::result::Result;

use serde_derive::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use ipnet::IpNet;
use rand::{thread_rng, Rng};
use ping_result::PingResult;
use hashbrown::HashMap;
use parking_lot::RwLock;

#[derive(Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "fping_exporter")]
struct Opt {
    /// Address to listen on for web interface and telemetry.
    #[structopt(long = "listen-address", default_value = "0.0.0.0:9215")]
    web_listen_addr: String,

    #[structopt(long = "targets")]
    targets: Vec<IpNet>,
}

struct ResultStore {
    ping_results: Arc<RwLock<HashMap<String, Vec<PingResult>>>>
}

fn process_subnet(target_subnet: IpNet) -> Result<Vec<PingResult>, String> {
    let subnet_string = format!("{:?}", target_subnet);

    let output = Command::new("/usr/local/sbin/fping")
        .args(&["-q", "-r", "0", "-c", "5", "-g", &subnet_string])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // fping uses '4' to indicate some issue with running the command
    if output.status.code() == Some(4) {
        return Err(stderr.to_string())
    }

    let mut results = vec![];

    for result in stderr.lines() {
        match result.parse() {
            Ok(ping_result) => results.push(ping_result),
            Err(e) => error!("{}", e),
        }
        // let ping_result: PingResult = result.parse().unwrap();
        // results.push(ping_result)
    }

    Ok(results)
}

fn index() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body("try <a href='/metrics'>/metrics</a>")
}

fn metrics(result_store: web::Data<ResultStore>) -> impl Responder {
    let mut output_string = String::new();

    // measurements (min, avg max)
    let ping_rtt = PrometheusMetric::new(
        "ping_rtt_seconds",
        MetricType::Gauge,
        "Ping round trip time in seconds",
    );

    output_string.push_str(&ping_rtt.render_header());
    let ping_results = &*result_store.ping_results.read();

    for (_target, results) in ping_results.iter() {
        for result in results {
            if result.minimum.is_none() {
                continue;
            }
            let ip = result.ip_address.to_owned().to_string();

            let mut attributes = Vec::new();
            attributes.push(("address", &ip[..]));
            attributes.push(("sample", "minimum"));
            output_string
                .push_str(&ping_rtt.render_sample(Some(&attributes), result.minimum.unwrap()));

            attributes = Vec::new();
            attributes.push(("address", &ip[..]));
            attributes.push(("sample", "average"));
            output_string
                .push_str(&ping_rtt.render_sample(Some(&attributes), result.average.unwrap()));

            attributes = Vec::new();
            attributes.push(("address", &ip[..]));
            attributes.push(("sample", "maxiumum"));
            output_string
                .push_str(&ping_rtt.render_sample(Some(&attributes), result.maxiumum.unwrap()));
        }
    }

    output_string.push_str("\n\n");

    // packets lost as a percentage
    let ping_packet_loss = PrometheusMetric::new(
        "ping_packet_loss_percent",
        MetricType::Gauge,
        "Percent of ping packets lost",
    );
    output_string.push_str(&ping_packet_loss.render_header());

    for (_target, results) in ping_results.iter() {
        for result in results {
            let ip = result.ip_address.to_owned().to_string();
            let mut attributes = Vec::new();
            attributes.push(("address", &ip[..]));
            output_string
                .push_str(&ping_packet_loss.render_sample(Some(&attributes), result.lost));
        }
    }

    HttpResponse::Ok()
        .body(output_string)
}

fn main() {
    simple_logger::init().unwrap();

    let file_contents = fs::read_to_string("fping_exporter.toml")
        .expect("Something went wrong reading the file");
    let options = Opt::from_args_with_toml(&file_contents).expect("toml parse failed");

    let results = Arc::new(RwLock::new(HashMap::new()));
    let result_store = web::Data::new(ResultStore {
        ping_results: results.clone()
    });

    let targets = Box::new(options.targets);
    let static_targets: &'static Vec<IpNet> = Box::leak(targets);

    // background threads to do the pings
    for target in static_targets {
        let results_arc = results.clone();
        thread::spawn(move || {

            // offset fping commands by some random amount of time
            let mut rng = thread_rng();
            let n = rng.gen_range(0, 60);
            thread::sleep(Duration::from_secs(n));

            loop {
                debug!("running {}", target);
                let now = Instant::now();

                match process_subnet(*target) {
                    Ok(subnet_results) => {
                        let mut global_results = results_arc.write();
                        global_results.remove(&target.to_string());
                        global_results.insert(target.to_string(), subnet_results);
                    },
                    Err(e) => {
                        error!("error {}", e);
                        ::std::process::exit(4);
                    },
                }

                // only run once per minute
                thread::sleep(Duration::from_secs(60 - now.elapsed().as_secs()));
            }
        });
    }

    // start metrics server
    HttpServer::new(move || {
        App::new()
            .register_data(result_store.clone())
            .wrap(middleware::Compress::default())
            .route("/", web::get().to(index))
            .route("/metrics", web::get().to(metrics))
    })
    .bind(options.web_listen_addr)
    .unwrap()
    .run()
    .unwrap();
}

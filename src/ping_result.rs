use std::str::FromStr;
use std::net::IpAddr;
use regex::Regex;

use snafu::{OptionExt, ResultExt, Snafu};
use lazy_static::lazy_static;

lazy_static! {
    static ref FPING_REGEX: Regex = Regex::new(
    	r"(?P<ip_address>.*) :.*= (?P<sent>\d+)/(?P<received>\d+)/(?P<lost>\d+)%(?:,.*= (?P<min>\d+\.?\d*)/(?P<avg>\d+\.?\d*)/(?P<max>\d+\.?\d*))?"
    ).unwrap();
}

#[derive(Debug, Snafu)]
pub enum FpingParseError {
	#[snafu(display("Unable to parse fping output"))]
	CaptureRegex,

	#[snafu(display("Unable to find `{}` field in fping output", name))]
    MissingField { name: String },

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

#[derive(Debug)]
pub struct PingResult {
    pub ip_address: IpAddr,
    pub sent: u8,
    pub received: u8,
    pub lost: u8,
    pub minimum: Option<f64>,
    pub average: Option<f64>,
    pub maxiumum: Option<f64>,
}

impl FromStr for PingResult {
	type Err = FpingParseError;

    fn from_str(ping_result: &str) -> Result<Self, Self::Err> {
        let caps = FPING_REGEX.captures(&ping_result).unwrap();

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

       	Ok(PingResult {
            ip_address,
            sent,
            received,
            lost,
            minimum,
            average,
            maxiumum,
        })
    }
}
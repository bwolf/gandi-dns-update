use log::{debug, info, trace};
use std::env;
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use std::{error, fmt};

use trust_dns_resolver::config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts};
use trust_dns_resolver::lookup::Lookup;
use trust_dns_resolver::proto::rr::{RData, Record, RecordType};
use trust_dns_resolver::proto::xfer::DnsRequestOptions;
use trust_dns_resolver::{TokioAsyncResolver, error::ResolveError, TokioHandle};

mod gandi_client;

use gandi_client::GandiClient;

type Resolver = TokioAsyncResolver;

fn resolver(
    config: ResolverConfig,
    options: ResolverOpts
) -> Result<Resolver, ResolveError> {
    Resolver::new(config, options, TokioHandle)
}

static DNS_TIMEOUT: Duration = Duration::from_secs(15);
static HTTP_TIMEOUT: Duration = Duration::from_secs(15);

macro_rules! crate_name {
    () => {
        env!("CARGO_PKG_NAME")
    };
}

#[derive(Debug)]
struct AppConfig {
    gandi_api_key: String,
    domain_ip: Option<Ipv4Addr>,
    domain_fqdn: String,
    domain_dynamic_items: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let gandi_api_key = env::var("GANDI_API_KEY").expect("GANDI_API_KEY env-var is present");
        let domain_ip = env::var_os("DOMAIN_IP").map(|os| os.into_string().unwrap());
        let domain_ip: Option<Ipv4Addr> = domain_ip.map(|s| s.parse().expect("Valid Ipv4Addr"));
        let domain_fqdn = env::var("DOMAIN_FQDN").expect("DOMAIN_FQDN env-var is present");
        let domain_dynamic_items =
            env::var("DOMAIN_DYNAMIC_ITEMS").expect("DOMAIN_DYNAMIC_ITEMS env-var is present");

        if !domain_fqdn.ends_with('.') {
            panic!(
                "Configuration entry `domain_fqdn` does not end with '.': {}",
                domain_fqdn
            );
        }

        let domain_dynamic_items: Vec<String> = domain_dynamic_items
            .split(',')
            .map(|s| s.to_string())
            .collect();

        Self {
            gandi_api_key,
            domain_ip,
            domain_fqdn,
            domain_dynamic_items,
        }
    }
}

#[derive(Debug)]
struct AppError {
    msg: String,
}

impl AppError {
    fn new(msg: &str) -> Self {
        Self { msg: msg.into() }
    }
}

impl error::Error for AppError {
    fn description(&self) -> &str {
        "Application error"
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DNS error {}", self.msg)
    }
}

impl From<String> for AppError {
    fn from(s: String) -> AppError {
        AppError { msg: s }
    }
}

impl From<ResolveError> for AppError {
    fn from(error: ResolveError) -> AppError {
        From::from(format!("Resolve error: {}", error))
    }
}

fn ns_of_record(record: &Record) -> Option<String> {
    match record.rdata() {
        RData::NS(name) => Some(name.to_utf8()),
        _ => None,
    }
}

fn ipv4_of_record(record: &Record) -> Option<Ipv4Addr> {
    match record.rdata() {
        RData::A(ip) => Some(*ip),
        _ => None,
    }
}

async fn dns_lookup(
    resolver: &Resolver,
    name: String,
    rr_type: RecordType,
) -> Result<Record, AppError> {
    let lookup: Lookup = resolver
        .lookup(name, rr_type, DnsRequestOptions::default())
        .await?;

    let res: Option<Record> = lookup.record_iter().find_map(|rec| {
        if rec.rr_type() == rr_type {
            Some(rec.clone())
        } else {
            None
        }
    });

    res.ok_or_else(|| {
        let msg: String = format!("Record type {} not found", rr_type);
        AppError::new(&msg)
    })
}

fn resolver_opts_with_timeout() -> ResolverOpts {
    ResolverOpts {
        timeout: DNS_TIMEOUT,
        use_hosts_file: false,
        ..Default::default()
    }
}

async fn whats_my_ip(bootstrap_resolver: &Resolver) -> Result<Ipv4Addr, AppError> {
    let resolver_record = dns_lookup(
        bootstrap_resolver,
        "resolver1.opendns.com.".into(),
        RecordType::A,
    )
    .await?;

    let resolver_ip =
        ipv4_of_record(&resolver_record).ok_or_else(|| AppError::new("No IPv4 record found"))?;

    let ns_config = NameServerConfig {
        protocol: Protocol::Udp,
        socket_addr: SocketAddr::new(IpAddr::V4(resolver_ip), 53),
        tls_dns_name: None,
        trust_nx_responses: true,
    };

    let resolver_config = ResolverConfig::from_parts(
        Some(resolver_record.name().clone()),
        vec![],
        vec![ns_config],
    );

    let resolver = resolver(resolver_config, resolver_opts_with_timeout())?;

    let my_ip_record = dns_lookup(&resolver, "myip.opendns.com".into(), RecordType::A).await?;

    ipv4_of_record(&my_ip_record).ok_or_else(|| AppError::new("No IPv4 record found"))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
        match std::env::var("RUST_LOG") {
        Ok(_) => {}
        Err(_) => {
            let logger = crate_name!().replace("-", "_");
            std::env::set_var("RUST_LOG", format!("info,{}=debug", logger));
        }
    }
    env_logger::init();

    let config = AppConfig::from_env();
    let google_dns = resolver(ResolverConfig::google(), resolver_opts_with_timeout())?;
    let gandi = GandiClient::new(config.gandi_api_key, HTTP_TIMEOUT);

    // Which IP address to use for updating domain records.
    let my_ip = match config.domain_ip {
        Some(ip) => {
            info!("Using given IP address {}", ip);
            ip
        }
        None => {
            // Initially get my external IP address
            info!("Looking up my IP address");
            whats_my_ip(&google_dns).await?
        }
    };
    info!("My IP address is {}", my_ip);

    for domain_dynamic_item in &config.domain_dynamic_items {
        info!(
            "Processing domain name {}, record {}",
            &config.domain_fqdn, domain_dynamic_item
        );

        // Determine the domains authoritative name server IP address
        // and use this to construct a resolver to query this NS.
        let domain_record =
            dns_lookup(&google_dns, config.domain_fqdn.clone(), RecordType::NS).await?;
        let domain_fqdn: String = domain_record.name().to_utf8();
        trace!("Domain {} DNS INFO {:?}", domain_fqdn, domain_record);

        // Get name of authoritative NS
        let domain_ns = ns_of_record(&domain_record).expect("Cannot get NS record");
        debug!("Domain {} first NS name is {}", domain_fqdn, domain_ns);

        // Get the IP address of the authoritative NS
        let domain_ns_a = dns_lookup(&google_dns, domain_ns, RecordType::A).await?;
        let domain_ns_ip = ipv4_of_record(&domain_ns_a).expect("Cannot get A record");
        debug!("Domain {} NS IP {}", domain_fqdn, domain_ns_ip);

        // Construct a resolver to query this NS
        let ns_config = NameServerConfig {
            protocol: Protocol::Udp,
            socket_addr: SocketAddr::new(IpAddr::V4(domain_ns_ip), 53),
            tls_dns_name: None,
            trust_nx_responses: true,
        };
        let domain_resolver_config =
            ResolverConfig::from_parts(Some(domain_record.name().clone()), vec![], vec![ns_config]);

        let domain_resolver = resolver(domain_resolver_config, ResolverOpts::default())?;

        // Check the dynamic DNS record using this resolver
        let dynamic_record_name = format!("{}.{}", domain_dynamic_item, domain_fqdn);
        info!(
            "Checking domain {} dynamic item {}",
            domain_fqdn, &dynamic_record_name
        );

        let dynamic_record =
            dns_lookup(&domain_resolver, dynamic_record_name.clone(), RecordType::A).await?;
        trace!("Dynamic domain {} record {:?}", domain_fqdn, dynamic_record);
        let dynamic_ip = ipv4_of_record(&dynamic_record).expect("Cannot get IPv4 record");

        if dynamic_ip != my_ip {
            info!(
                "Dynamic domain {} record {} needs update: {} != {}",
                domain_fqdn, &dynamic_record_name, dynamic_ip, my_ip
            );

            let domain_fqdn_without_dot = domain_fqdn.trim_end_matches('.');

            gandi
                .update_a_record(
                    domain_fqdn_without_dot,
                    domain_dynamic_item,
                    &my_ip.to_string(),
                    Duration::from_secs(300).into(),
                )
                .await?;
        } else {
            info!(
                "Dynamic domain {} record {} is up to date: {}",
                domain_fqdn, &dynamic_record_name, dynamic_ip
            );
        }
    }

    Ok(())
}

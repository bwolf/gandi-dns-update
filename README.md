# gandi-dns-update

[`gandi-dns-update`](https://github.com/bwolf/gandi-dns-update) is a DNS updater for [Gandi.net](https://gandi.net/), which is ideal for use in container environments. It follows the principles of [twelve-factor app](https://12factor.net). Whenever possible, networking timeouts are used to avoid hanging the application.


## Principle of Operation

1. Don't rely on the system resolver, as DNS requests maybe proxied
2. Use Google DNS to lookup the NS of `resolver1.opendns.com`
3. Determine the current dynamic IP
    1. Use `myip.opendns.com` to lookup the current dynamic IP
    2. Alternatively, if `DOMAIN_IP` is given, disable the dynamic lookup and use this IP address
4. Use Google DNS to lookup the NS of the given domain (hosted with Gandi.net)
5. For each given dynamic item, lookup the (A) record in the Gandi NS and compare it against the current dynamic IP. Update it if it does not match
6. Update DNS (A) record at Gandi, using the Gandi Live DNS API

Network Timeouts (currently not configurable):
- DNS lookup: 15 seconds
- HTTP methods: 15 seconds

Commentary:

I was in the need for a tool like this for a longer time, and although there seem many projects like this one, most are either unmaintained, use either Python or Go (fill in arbitrary programming language here) in some unsupported older version, use configuration files, only use the system resolver, or don't use proper networking timeouts. At some point I wrote a quick and dirty sketch in Python (using `dns-lexicon`) which worked (besides timeouts), but the container image had a size of 70 MiB. Running it in Kubernetes every 5 minutes 24/7 revealed that sometimes the jobs hangs because of a race conditions in the DNS resolver logic.


### Limitations
- only IPv4 is supported
- only Gandi is supported


## Configuration

The following environment variables must be given:

- `GANDI_API_KEY` :: Gandi Live DNS API key
- `DOMAIN_IP` :: Optionally disable current dynamic IP lookup and use this IP address
- `DOMAIN_FQDN` :: Domain to be managed, ending with a dot '.'
- `DOMAIN_DYNAMIC_ITEMS` :: List of entries within a domain to be updated. For example  'a' or 'a,b' will process the A records `a.domain.tld` and respectively `b.domain.tld` if `domain.tld` is given as `DOMAIN_FQDN`

NOTE: the domain must be fully qualified and needs to end with a dot '.'. The program will panic, if not full-filled.

## Examples

Example usage as container:

``` shell
docker run --rm \
       -e GANDI_API_KEY=your-api-key \
       -e DOMAIN_FQDN=domain.tld. -e DOMAIN_DYNAMIC_ITEMS=a,b,c \
       image-name:latest
```

Example usage with Cron:

``` shell
*/5 * * * * /usr/bin/env -i GANDI_API_KEY=your-api-key DOMAIN_FQDN=domain.tld. DOMAIN_DYNAMIC_ITEMS=a,b,c /path/to/gandi-dns-update
```

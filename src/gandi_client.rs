use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::boxed::Box;
use std::error::Error;
use std::time::Duration;
use reqwest::header;

static GANDI_LIVE_DNS_BASE_URL: &str = "https://dns.api.gandi.net/api/v5";

// Used for requests and responses of the Gandi live API V5.
// For requests mostly (ttl, values) is used.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct GandiRRSet {
    #[serde(rename = "rrset_type", skip_serializing_if = "Option::is_none")]
    r#type: Option<String>,
    #[serde(rename = "rrset_ttl")]
    ttl: u64,
    #[serde(rename = "rrset_name", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(rename = "rrset_values")]
    values: Vec<String>,
}

#[derive(Debug)]
pub struct Ttl {
    secs: u64,
}

impl From<Duration> for Ttl {
    fn from(d: Duration) -> Self {
        Self { secs: d.as_secs() }
    }
}

#[derive(Debug)]
pub struct GandiClient {
    api_key: String,
    timeout: Duration,
}

impl GandiClient {
    pub fn new(api_key: String, timeout: Duration) -> Self {
        GandiClient { api_key, timeout }
    }

    pub async fn update_a_record(
        &self,
        domain: &str,
        name: &str,
        value: &str,
        ttl: Ttl,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // curl -X PUT -H "Content-Type: application/json" \
        //   -H "X-Api-Key: $APIKEY" \
        //   -d '{"rrset_ttl": 10800,
        //        "rrset_values":["<VALUE>"]}' \
        //   https://dns.api.gandi.net/api/v5/domains/<DOMAIN>/records/<NAME>/<TYPE>
        if domain.ends_with('.') {
            return Err(From::from(
                "Domain in Gandi live API request must not end with '.'",
            ));
        }
        if name.contains('.') {
            return Err(From::from("Record name must not contain '.'"));
        }

        let uri = format!(
            "{}/domains/{}/records/{}/A",
            GANDI_LIVE_DNS_BASE_URL, domain, name
        );

        let request_body = GandiRRSet {
            r#type: None,
            ttl: ttl.secs,
            name: None,
            values: vec![value.into()],
        };

        let request_body = serde_json::to_string(&request_body)?;

        debug!("Posting to {}, body {}", uri, request_body);

        let client = reqwest::Client::new();
        let response = client.put(&uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Api-Key", &self.api_key)
            .timeout(self.timeout)
            .body(request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await?;
            let msg = format!("Gandi request failed, response is: {}", text);
            return Err(From::from(msg));
        } else {
            info!("Gandi update successful");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::GandiRRSet;
    use serde_json::json;

    #[test]
    fn rrset_request_serializes_ok() {
        let input = GandiRRSet {
            r#type: Some("aType".into()),
            ttl: 666,
            name: Some("aName".into()),
            values: vec![String::from("value1"), String::from("value2")],
        };
        let actual = serde_json::to_string(&input).unwrap();
        let expected = r#"{"rrset_type":"aType","rrset_ttl":666,"rrset_name":"aName","rrset_values":["value1","value2"]}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn rrset_request_null_values_omitted_serializes_ok() {
        let input = GandiRRSet {
            r#type: None,
            ttl: 666,
            name: None,
            values: vec![String::from("value1")],
        };
        let actual = serde_json::to_string(&input).unwrap();
        let expected = r#"{"rrset_ttl":666,"rrset_values":["value1"]}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn rrset_response_deserializes_ok() {
        let actual = json!({"rrset_ttl":666,"rrset_values":["value1","value2"]});
        let actual = serde_json::from_value(actual).unwrap();
        let expected = GandiRRSet {
            r#type: None,
            ttl: 666,
            name: None,
            values: vec![String::from("value1"), String::from("value2")],
        };
        assert_eq!(expected, actual);
    }
}

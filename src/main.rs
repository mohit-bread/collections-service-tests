use std::collections::HashMap;
use std::str::FromStr;

use chrono::{Duration, SecondsFormat};
use chrono::prelude::Utc;
use const_format::concatcp;
use log::{debug, LevelFilter};
use rand::Rng;
use reqwest::{Body, Client, ClientBuilder, header, Method, Request, Url};
use reqwest::header::HeaderName;
use serde_json::json;
use simple_logger::SimpleLogger;
use uuid::Uuid;

use crate::header::{HeaderMap, HeaderValue};

const AUTH_APP_ID: &'static str = "9132d63e-78fd-4ee4-bde2-68583a4657b8";
const PROGRAM_ID: &'static str = "b7beac2e-2714-4d1d-b110-72e405b41c34";
const MERCHANT_ID: &'static str = "0020fa5a-f107-4ba7-b7f0-d869ea0eed07";
//const CURRENCY: &'static str = "USD";

//const BASE_URL: &str = "https://api.slice-mk-ads.slice.ue2.breadgateway.net";
const BASE_URL: &str = "https://api.npp-dev-ads.ue2.breadgateway.net";
const SEND_CODE_URL: &str = concatcp!(BASE_URL, "/api/auth/send-code");
const BUYER_AUTH_URL: &str = concatcp!(BASE_URL, "/api/auth/buyer/authorize");
const BUYER_URL: &str = concatcp!(BASE_URL, "/api/buyer");
const APPLICATION_URL: &str = concatcp!(BASE_URL, "/api/application");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    SimpleLogger::new().with_level(LevelFilter::Debug).init().unwrap();
    let random_no: u128 = rand::thread_rng().gen();
    let request_id = Uuid::from_u128(random_no);
    let request_id = request_id.to_hyphenated().to_string();
    let request_id = request_id.as_str();
    debug!("RequestId:{}", request_id);
    let random_no = random_no.to_string();
    let random_no_email_and_phone = &random_no[5..15];
    let email_id = format!("{}@domain.com", random_no_email_and_phone);
    let email_id = email_id.as_str();
    let phone = format!("+1{}", random_no_email_and_phone);
    let phone = phone.as_str();
    let client = ClientBuilder::new().danger_accept_invalid_certs(true).build().unwrap();
    let reference_id = send_code(&client, phone, email_id, request_id).await?;
    debug!("ReferenceId:{}", reference_id);
    let anonymous_jwt_token = authorize_buyer(&client, reference_id.as_str(), request_id).await?;
    let anonymous_jwt_token = anonymous_jwt_token.as_str();
    debug!("AnonymousToken:{}", anonymous_jwt_token);
    let (buyer_id, buyer_jwt_token, contact_id) = create_buyer(&client, anonymous_jwt_token, phone, email_id, request_id).await?;
    let buyer_id = buyer_id.as_str();
    let buyer_jwt_token = buyer_jwt_token.as_str();
    debug!("BuyerId:{} Token:{} ContactId:{}" , buyer_id, buyer_jwt_token, contact_id);
    update_buyer_contact(&client, request_id, buyer_id, buyer_jwt_token, contact_id.as_str(), phone, email_id).await?;
    let (application_id, payment_agreement_id) = create_application(&client, request_id, buyer_jwt_token).await?;
    println!("ApplicationId:{} PaymentAgreementId:{}", application_id, payment_agreement_id);
    Ok(())
}

async fn send_code(client: &Client, phone: &str, email: &str, request_id: &str) -> Result<String, Box<dyn std::error::Error>> {
    let dt = Utc::now() + Duration::days(-1);
    let dt = dt.to_rfc3339_opts(SecondsFormat::Secs, true);
    let dt = dt.as_str();
    let send_code_json = json!({"deliveryMethod": "SMS", "phone": phone, "email": email,
    "disclosures": [ { "type": "privacy-policy-choices", "acceptedAt": dt}, {"type": "terms-of-use", "acceptedAt": dt } ], "uat": { "auth": { "token": "1234" }}});
    let mut headers: HashMap<&str, &str> = HashMap::new();
    headers.insert("x-api-version", "v2");
    let (_, ref_id_json) = call(&client, request_id, Method::POST, SEND_CODE_URL, Some(&send_code_json), Some(headers)).await?;
    debug!("Response JSON-{}", ref_id_json);
    Ok(ref_id_json["referenceID"].as_str().unwrap().to_string())
}

async fn authorize_buyer(client: &Client, ref_id: &str, request_id: &str) -> Result<String, Box<dyn std::error::Error>> {
    let authorize_buyer_json = json!({"credentials": {"code": "1234"}, "referenceID": ref_id, "merchantID": MERCHANT_ID, "programID": PROGRAM_ID});
    let headers: HashMap<&str, &str> = HashMap::new();
    let (_, auth_buyer_json) = call(&client, request_id, Method::POST, BUYER_AUTH_URL, Some(&authorize_buyer_json), Some(headers)).await?;
    debug!("Response JSON-{}", auth_buyer_json);
    Ok(auth_buyer_json["token"].as_str().unwrap().to_string())
}

async fn create_buyer(client: &Client, anonymous_jwt_token: &str, phone: &str, email: &str, request_id: &str) -> Result<(String, String, String), Box<dyn std::error::Error>> {
    let new_buyer_json = json!({
    "identity": {
            "birthDate": "1991-10-09",
            "name": {
                "additionalName": "One",
                "familyName": "Bread",
                "givenName": "Athens",
            },
            "email": email,
            "phone": phone,
            "iinShort": "1234",
        },
        "languagePreference": "en-us"
    });
    let mut headers: HashMap<&str, &str> = HashMap::new();
    let anonymous_token = format!("Bearer {}", anonymous_jwt_token);
    let anonymous_token = anonymous_token.as_str();
    headers.insert(header::AUTHORIZATION.as_str(), anonymous_token);
    let (header_map, new_buyer_json_res) = call(&client, request_id, Method::POST, BUYER_URL, Some(&new_buyer_json), Some(headers)).await?;
    let buyer_jwt_token = header_map[header::AUTHORIZATION].to_str().unwrap();
    let buyer_jwt_token = String::from(buyer_jwt_token);
    debug!("Response Buyer-{}", new_buyer_json_res);
    Ok((new_buyer_json_res["id"].as_str().unwrap().to_string(), buyer_jwt_token, String::from(get_contact_id(&new_buyer_json_res))))
}

async fn update_buyer_contact(client: &Client, request_id: &str, buyer_id: &str, buyer_jwt_token: &str, contact_id: &str, phone: &str, email: &str) -> Result<(), Box<dyn std::error::Error>> {
    let buyer_contact_json = json!({
        "name": {
            "additionalName": "One",
            "familyName": "Bread",
            "givenName": "Athens"
        },
        "address": {
            "address1": "78-22 88th Avenue",
            "address2": "",
            "locality": "New York",
            "region": "NY",
            "postalCode": "10028",
            "country": "US"
        },
        "email": email,
        "phone": phone
    });
    let mut headers: HashMap<&str, &str> = HashMap::new();
    let auth_token = format!("Bearer {}", buyer_jwt_token);
    let auth_token = auth_token.as_str();
    headers.insert(header::AUTHORIZATION.as_str(), auth_token);
    let buyer_contact_url = format!("{}/api/buyer/{}/contact/{}", BASE_URL, buyer_id, contact_id);
    call(&client, request_id, Method::PUT, buyer_contact_url.as_str(), Some(&buyer_contact_json), Some(headers)).await?;
    Ok(())
}

async fn create_application(client: &Client, request_id: &str, buyer_jwt_token: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let dt = Utc::now() + Duration::days(-1);
    let dt = dt.to_rfc3339_opts(SecondsFormat::Secs, true);
    let dt = dt.as_str();
    let application_json = json!({
    "productType": "LOAN",
    "order": {
       "orderitem": [
         {"name":"MK Cycle","quantity":1,"unitPrice":{ "currency": "USD", "value":100000 },"unitTax":{ "currency": "USD", "value":0 }, "shippingCost":{ "currency": "USD", "value":0 }}
        ],
        "subTotal": { "currency": "USD", "value":100000 },
        "totalTax": { "currency": "USD", "value":10000 },
        "totalPrice": { "currency": "USD", "value":110000 },
        "totalShipping": { "currency": "USD", "value":0 },
        "totalDiscounts": { "currency": "USD", "value":0 }
        },
        "disclosures": [
            {"type":"SOFT_PULL", "acceptedAt": dt}
        ]
        });
    let mut headers: HashMap<&str, &str> = HashMap::new();
    let auth_token = format!("Bearer {}", buyer_jwt_token);
    let auth_token = auth_token.as_str();
    headers.insert(header::AUTHORIZATION.as_str(), auth_token);
    headers.insert("X-API-Version", "v2");
    let (_, application_json) = call(&client, request_id, Method::POST, APPLICATION_URL, Some(&application_json), Some(headers)).await?;
    let app_id = application_json["id"].as_str().unwrap().to_string();
    let agreement_id = application_json["paymentAgreements"][0]["id"].as_str().unwrap().to_string();
    Ok((app_id, agreement_id))
}

async fn call(client: &Client, request_id: &str, method: Method, url: &str, json: Option<&serde_json::Value>, headers: Option<HashMap<&str, &str>>) -> Result<(HeaderMap, serde_json::Value), Box<dyn std::error::Error>> {
    let mut request = Request::new(method, Url::from_str(url).unwrap());
    let header_map: &mut HeaderMap<HeaderValue> = request.headers_mut();
    header_map.append(header::ACCEPT, "application/json".parse().unwrap());
    header_map.append(header::CONTENT_TYPE, "application/json".parse().unwrap());
    header_map.append(HeaderName::from_lowercase(b"x-bread-app-id").unwrap(), AUTH_APP_ID.parse().unwrap());
    header_map.append(HeaderName::from_lowercase(b"x-request-id").unwrap(), request_id.parse().unwrap());
    if headers.is_some() {
        for (key, val) in headers.unwrap() {
            header_map.append(HeaderName::from_str(key).unwrap(), HeaderValue::from_str(val).unwrap());
        }
    }
    if json.is_some() {
        let body = request.body_mut();
        *body = Some(Body::from(serde_json::to_string(json.unwrap()).unwrap()));
    }
    debug!("RequestHeaders:{:?}", request.headers());
    debug!("RequestBody:{}", std::str::from_utf8(request.body().unwrap().as_bytes().unwrap()).unwrap());
    let resp = client.execute(request).await?;
    let headers = resp.headers().clone();
    debug!("Response Headers:{:?}", headers);
    let resp_json: serde_json::Value = resp.json().await?;
    debug!("Response Body:{:?}", resp_json);
    Ok((headers, resp_json))
}

fn get_contact_id(buyer_json: &serde_json::Value) -> &str {
    let contacts = &buyer_json["contacts"];
    let contacts = contacts.as_object().unwrap();
    let contact_id = contacts.iter().next().unwrap().0;
    println!("{:?}", contact_id);
    contact_id
}
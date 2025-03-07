// Copyright (C) 2025  Jimmy Aguilar Mena

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![allow(dead_code)]

use regex;
use reqwest::{header, Client, Method, StatusCode, Url};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use log::{info, error, warn};

#[derive(Debug, Error)]
pub enum AlpacaError {
    #[error("Invalid API key or secret format")]
    InvalidKeyFormat,
    #[error("HTTP error {status}: {message}")]
    HttpError { status: StatusCode, message: String },
    #[error("Request error: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Timeout error")]
    Timeout,
    #[error("Other error: {0}")]
    Other(String),
}

pub struct AlpacaClient {
    pub(crate) base_url: String,
    pub(crate) data_url: String,
    pub(crate) headers: header::HeaderMap,
    pub(crate) client: Client,
    pub(crate) info: Value
}

impl AlpacaClient {
    pub async fn connect(api_key: &str, api_secret: &str) -> Result<Self, AlpacaError> {
        if !Self::validate_keys(&api_key, &api_secret) {
            return Err(AlpacaError::InvalidKeyFormat);
        }

        let mut headers = header::HeaderMap::with_capacity(3);
        headers.insert(
            "APCA-API-KEY-ID",
            header::HeaderValue::from_str(&api_key).map_err(|_| AlpacaError::InvalidKeyFormat)?,
        );
        headers.insert(
            "APCA-API-SECRET-KEY",
            header::HeaderValue::from_str(&api_secret).map_err(|_| AlpacaError::InvalidKeyFormat)?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut alpaca = Self {
            base_url: "https://paper-api.alpaca.markets".to_string(),
            data_url: "https://data.alpaca.markets".to_string(),
            headers,
            client: Client::builder().build()?,
            info: Value::Null
        };

        alpaca.info = alpaca.get_account().await?;
        info!("Alpaca API client initialized successfully");

        Ok(alpaca)
    }

    pub(crate) fn validate_keys(api_key: &str, api_secret: &str) -> bool {
        let key_re = regex::Regex::new(r"^(PK|AK)[A-Z0-9]{10,}$").unwrap();
        let secret_re = regex::Regex::new(r"^[A-Za-z0-9]{40,}$").unwrap();
        key_re.is_match(api_key) && secret_re.is_match(api_secret)
    }

    pub(crate) async fn make_request(
        &self,
        method: Method,
        endpoint: &str,
        base_url: &str,
        query: Option<&impl Serialize>,
        body: Option<&impl Serialize>,
        timeout: Option<std::time::Duration>
    ) -> Result<Value, AlpacaError> {

        let url = Url::parse(
                &format!("{}{}", base_url, endpoint)
            ).map_err(|e| AlpacaError::Other(e.to_string()))?;

        let mut request =
            self.client
                .request(method.clone(), url)
                .headers(self.headers.clone())
                .timeout(timeout.unwrap_or(std::time::Duration::from_secs(30)));

        if let Some(query) = query {
            request = request.query(query);
        }
        if let Some(body) = body {
            request = request.json(body);
        }

        info!("Request: {} {}", method, endpoint);

        let response = request
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AlpacaError::Timeout
                } else if e.is_connect() {
                    AlpacaError::ConnectionError(e.to_string())
                } else {
                    AlpacaError::RequestError(e)
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            if status == StatusCode::TOO_MANY_REQUESTS {
                warn!("Rate limit exceeded. Consider implementing backoff.");
            }
            return Err(AlpacaError::HttpError { status, message });
        }

        let json = response.json().await?;
        Ok(json)
    }

    pub async fn get_account(&self) -> Result<Value, AlpacaError> {
        self.make_request(
                Method::GET,
                "/v2/account",
                &self.base_url,
                None::<&()>,
                None::<&()>,
                Some(std::time::Duration::from_secs(10)),
            )
            .await
            .map_err(|e| {
                error!("Failed to get account information: {}", e);
                e
            })
    }

    pub async fn get_positions(&self) -> Result<Value, AlpacaError> {
        self.make_request(
                Method::GET,
                "/v2/positions",
                &self.base_url,
                None::<&()>,
                None::<&()>,
                None,
            )
            .await
            .map_err(|e| {
                error!("Failed to get positions: {}", e);
                e
            })
    }

    pub async fn place_order(
        &self,
        symbol: &str,
        qty: i64,
        side: &str,
        order_type: Option<&str>,
        time_in_force: Option<&str>,
    ) -> Result<Value, AlpacaError> {

        #[derive(Serialize)]
        struct OrderData<'a> {
            symbol: &'a str,
            qty: i64,
            side: &'a str,
            #[serde(rename = "type")]
            type_: &'a str,
            time_in_force: &'a str,
        }

        let data = OrderData {
            symbol,
            qty,
            side,
            type_: order_type.unwrap_or("market"),
            time_in_force: time_in_force.unwrap_or("ioc"),
        };

        self.make_request(
                Method::POST,
                "/v2/orders",
                &self.base_url,
                None::<&()>,
                Some(&data),
                None,
            )
            .await
            .map_err(|e| {
                error!("Failed to place order for {}: {}", symbol, e);
                e
            })
    }

    pub async fn get_prices(
        &self,
        assets: &[&str],
        price_type: &str,
    ) -> Result<Value, AlpacaError> {
        let allowed_types = ["trades", "quotes", "bars"];
        if !allowed_types.contains(&price_type) {
            return Err(AlpacaError::Other(format!(
                "Invalid price type: {}. Allowed: {}",
                price_type,
                allowed_types.join(", ")
            )));
        }

        if assets.is_empty() {
            return Ok(Value::Object(serde_json::Map::new()));
        }

        #[derive(Serialize)]
        struct QueryParams {
            symbols: String,
        }

        let params = QueryParams {symbols: assets.join(","),};

        self.make_request(
                Method::GET,
                &format!("/v2/stocks/{}/latest", price_type),
                &self.data_url,
                Some(&params),
                None::<&()>,
                None,
            )
            .await
            .map_err(|e| {
                error!("Failed to get prices: {}", e);
                e
            })
    }

    pub async fn get_order_info(&self, id: &str) -> Result<Value, AlpacaError> {
        self.make_request(
                Method::GET,
                &format!("/v2/orders/{}", id),
                &self.base_url,
                None::<&()>,
                None::<&()>,
                None,
            )
            .await
            .map_err(|e| {
                error!("Failed to get order info: {}", e);
                e
            })
    }
}

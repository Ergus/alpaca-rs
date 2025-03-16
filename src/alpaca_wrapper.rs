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

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use tokio::runtime::Runtime;
use tokio::task::JoinSet;
use std::sync::atomic;

//use futures::future::join_all;
use log;

#[derive(Debug, Serialize, Deserialize)]
struct CompletePosition {
    #[serde(with = "crate::utils::arc_rwlock_hashmap")]
    positions: Arc<RwLock<HashMap<String, crate::utils::Position>>>,
    #[serde(with = "crate::utils::atomic_f64")]
    cash: crate::AtomicF64,
}

impl Default for CompletePosition {
    fn default() -> Self {
        Self {
            positions: Arc::new(RwLock::new(HashMap::new())),
            cash: crate::AtomicF64::default()
        }
    }
}

#[derive(Debug)]
struct AlpacaWrapper {
    client: Arc<crate::AlpacaClient>,
    assets: Vec<String>,
    runtime: Arc<Runtime>,

    // Using RwLock for better read concurrency where possible
    position: CompletePosition,
    last_prices: Arc<RwLock<HashMap<String, HashMap<String, Value>>>>,

    initial_position: Option<Arc<HashMap<String, crate::utils::Position>>>,
}

impl AlpacaWrapper {
    pub fn new(
        api_key: &str,
        api_secret: &str,
        assets: Vec<String>,
    ) -> Self {
        assert!(!assets.is_empty(), "Assets list cannot be empty");

        let client = Arc::new(crate::AlpacaClient::connect(api_key, api_secret).unwrap());
        // Create a multi-threaded runtime with default thread count
        let runtime = Arc::new(Runtime::new().unwrap());

        let mut wrapper = AlpacaWrapper {
            client,
            assets,
            runtime,
            position: CompletePosition::default(),
            last_prices: Arc::new(RwLock::new(HashMap::new())),
            initial_position: None,
        };

        // Initialize data
        wrapper.runtime.block_on(wrapper.update_cash_async());
        wrapper.runtime.block_on(wrapper.update_positions_async());
        wrapper.update_prices();

        // Store initial position
        wrapper.initial_position = Some(Arc::new(wrapper.position.positions.read().unwrap().clone()));

        wrapper
    }

    pub fn update_prices(&self) {
        let items = &["trades", "quotes", "bars"];

        // Execute all requests in parallel using Tokio
        let mut set = JoinSet::new();

        for item in items.into_iter() {
            let client = self.client.clone();
            let assets = self.assets.clone();

            set.spawn(async move {
                let assets_copy = assets;
                client.get_prices(&assets_copy, crate::PriceType::from_str(item).unwrap()).await
            }
            );
        }

        let last_prices = HashMap::new();

        let mut asset_prices = HashMap::new();
        for asset in self.assets.clone() {
            asset_prices.insert(asset.to_string(), HashMap::new());
        }

        while let Some(result) = self.runtime.block_on(set.join_next()) {

            match result.unwrap().unwrap() {
                Value::Object(type_map) => {
                    for (price_name, price_values) in type_map {
                        match price_values {
                            Value::Object(price_map) => {
                                for (asset_name, prices) in price_map {
                                    if self.assets.contains(&asset_name) {
                                        asset_prices.get_mut(asset_name.as_str())
                                            .unwrap()
                                            .insert(
                                                crate::PriceType::from_str(price_name.as_str()).unwrap(),
                                                prices
                                            );
                                    }
                                }
                            },
                            _ => println!("Entry is not a JSON object."),
                        }
                    }
                },
                _ => println!("Value is not a JSON object.")
            }
        }


        // Take write lock only to update the final result
        let mut prices_guard = self.last_prices.write().unwrap();
        *prices_guard = last_prices;
    }

    pub async fn get_order_info_async(&self, order_id: &str) -> Value {
        self.client.get_order_info_async(order_id).await.unwrap()
    }

    pub fn get_order_info(&self, order_id: &str) -> Value {
        self.runtime.block_on(self.client.get_order_info_async(order_id)).unwrap()
    }

    pub async fn update_positions_async(&self)
    {
        let positions = self.client.get_positions().await;

        let new_positions = positions
            .into_iter()
            .filter_map(|position| {
                let symbol = position["symbol"].as_str().unwrap().to_string();

                if !self.assets.contains(&symbol) {
                    return None;
                }

                let parse_value = |key: &str| -> f64 {
                    position[key]
                        .as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0)
                };

                Some((
                    symbol,
                    crate::utils::Position {
                        qty: parse_value("qty_available"),
                        value: parse_value("market_value"),
                        entry: parse_value("avg_entry_price"),
                        price: parse_value("current_price"),
                    },
                ))
            }).collect();

        // Update the shared positions with a write lock
        match self.position.positions.write() {
            Ok(mut positions_guard) => {
                *positions_guard = new_positions;
            },
            Err(e) => {
                log::error!("Failed to acquire write lock for positions: {}", e);
            }
        }
    }

    pub async fn update_cash_async(&self) {
        let cash = self.client
            .get_account_info()
            .await
            .expect("Couldn't get account info")
            .get("cash")
            .expect("Couldn't get cash from account info")
            .as_str()
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0);

        self.position.cash.store(cash, atomic::Ordering::Relaxed);
    }

}

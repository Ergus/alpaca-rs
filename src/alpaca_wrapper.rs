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

    // pub fn manage_buy_signal_async(&self, ticker: &str) -> Option<Value> {
    //     log::info!("Manage buy signal");

    //     // Get seller price
    //     let seller_price = {
    //         let prices_guard = self.last_prices.read().unwrap();
    //         prices_guard
    //             .get(ticker)
    //             .and_then(|asset_prices| asset_prices.get("quotes"))
    //             .and_then(|quotes| quotes["ap"].as_f64())
    //             .unwrap_or(0.0)
    //     };

    //     let cash = self.position.cash.load(atomic::Ordering::Relaxed);
    //     let qty = (cash / seller_price).floor() as i64;

    //     // Only buy if we have enough cash
    //     if qty > 0 {
    //         return Some(self.client.place_order_async(ticker, qty as i64, "buy", None, None).await);
    //     }

    //     None
    // }

    // pub fn manage_buy_signal(&self, ticker: &str) -> Option<Value> {
    //     self.runtime.block_on(self.manage_buy_signal_async(ticker))
    // }

    // pub async fn manage_sell_signal_async(&self, ticker: &str) -> Option<Value> {
    //     log::info!("Manage sell signal");

    //     // Get position information
    //     let (qty, entry_price) = {
    //         let positions_guard = self.positions.read().unwrap();
    //         if let Some(position) = positions_guard.get(ticker) {
    //             (position.qty, position.entry)
    //         } else {
    //             (0.0, 0.0)
    //         }
    //     };

    //     // Get buyer price
    //     let buyer_price = {
    //         let prices_guard = self.last_prices.read().unwrap();
    //         prices_guard
    //             .get(ticker)
    //             .and_then(|asset_prices| asset_prices.get("quotes"))
    //             .and_then(|quotes| quotes["bp"].as_f64())
    //             .unwrap_or(0.0)
    //     };

    //     // Only place the order if we hold some and bought them cheaper than current price
    //     if qty > 0.0 && buyer_price > entry_price {
    //         return Some(self.client.place_order_async(ticker, qty as i64, "sell", None, None).await);
    //     }

    //     None
    // }

    // pub fn manage_sell_signal(&self, ticker: &str) -> Option<Value> {
    //     self.runtime.block_on(self.manage_sell_signal_async(ticker))
    // }

    // Add this method to spawn background tasks for periodic updates
    // pub fn start_background_updates(&self, update_interval_ms: u64) {
    //     let last_prices = self.last_prices.clone();
    //     let positions = self.positions.clone();
    //     let cash = self.cash.clone();
    //     let client = self.client.clone();
    //     let assets = self.assets.clone();

    //     // Spawn a Tokio task for periodic updates
    //     self.runtime.spawn(async move {
    //         let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(update_interval_ms));

    //         loop {
    //             interval.tick().await;

    //             // Update prices (most time-sensitive)
    //             let items = vec!["trades", "quotes", "bars"];
    //             let price_futures: Vec<_> = items.iter().map(|item| {
    //                 let item_str = item.to_string();
    //                 let client_clone = client.clone();
    //                 let assets_clone = assets.clone();

    //                 async move {
    //                     let result = client_clone.get_prices(&assets_clone, &item_str).await;
    //                     (item_str, result)
    //                 }
    //             }).collect();

    //             let price_results = join_all(price_futures).await;

    //             // Process price results
    //             let mut results = HashMap::new();
    //             for (item, result) in price_results {
    //                 results.insert(item, result);
    //             }

    //             // Reshape results
    //             let mut new_prices = HashMap::new();
    //             for asset in &assets {
    //                 let mut asset_prices = HashMap::new();
    //                 for item in &items {
    //                     if let Some(item_data) = results.get(*item) {
    //                         if let Some(asset_data) = item_data.get(asset) {
    //                             asset_prices.insert(item.to_string(), asset_data.clone());
    //                         }
    //                     }
    //                 }
    //                 new_prices.insert(asset.clone(), asset_prices);
    //             }

    //             // Update last_prices
    //             {
    //                 let mut prices_guard = last_prices.write().unwrap();
    //                 *prices_guard = new_prices;
    //             }

    //             // Update positions (less frequently if desired)
    //             let positions_result = client.get_positions_async().await;
    //             let mut new_positions = HashMap::new();
    //             for position in positions_result {
    //                 let symbol = position["symbol"].as_str().unwrap_or_default().to_string();

    //                 if assets.contains(&symbol) {
    //                     new_positions.insert(symbol, Position {
    //                         qty: position["qty_available"].as_str().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0),
    //                         value: position["market_value"].as_str().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0),
    //                         entry: position["avg_entry_price"].as_str().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0),
    //                         price: position["current_price"].as_str().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0),
    //                     });
    //                 }
    //             }

    //             // Update positions
    //             {
    //                 let mut positions_guard = positions.write().unwrap();
    //                 *positions_guard = new_positions;
    //             }

    //             // Update cash
    //             let account = client.get_account_async().await;
    //             let new_cash = account["cash"].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);

    //             {
    //                 let mut cash_guard = cash.lock().unwrap();
    //                 *cash_guard = new_cash;
    //             }
    //         }
    //     });
    // }
}

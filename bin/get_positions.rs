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

use std::env;
use alpaca_rs::AlpacaClient;

#[tokio::main]
async fn main() -> Result<(),Box<dyn std::error::Error>>
{
    let client = AlpacaClient::connect(
            env::var("ALPACA_API_KEY").unwrap().as_str(),
            env::var("ALPACA_SECRET_KEY").unwrap().as_str())
        .await?;

    let positions = client.get_positions().await.unwrap();

    println!("Client {}", serde_json::to_string_pretty(&client).unwrap());

    println!("{}", serde_json::to_string_pretty(&positions).unwrap());

    Ok(())
}

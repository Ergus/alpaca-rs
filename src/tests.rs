#[cfg(test)]
mod tests {
    use crate::*;
    use serde::Serialize;
    use serde_json::{json,Value};
    use reqwest::StatusCode;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::http::{Method, HeaderValue, HeaderMap};
    use wiremock::matchers::{method, path, header, query_param};

    // Helper function to create a test client with mocked URLs
    async fn create_test_client(
        mock_base_url: &str,
        mock_data_url: &str
    ) -> AlpacaClient {
        let api_key = "PKTEST12345ABCDEFGHI";
        let api_secret = "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFG";

        // Override the account endpoint for client initialization
        let mock_account_response = json!({
            "id": "test-account-id",
            "status": "ACTIVE"
        });

        let mut headers = HeaderMap::with_capacity(3);
        headers.insert(
            "APCA-API-KEY-ID",
            HeaderValue::from_str(api_key).unwrap(),
        );
        headers.insert(
            "APCA-API-SECRET-KEY",
            HeaderValue::from_str(api_secret).unwrap(),
        );
        headers.insert(
            "CONTENT_TYPE",
            HeaderValue::from_static("application/json"),
        );

        let client = reqwest::Client::builder().build().unwrap();

        // We need to create a client manually since we're not calling the real API
        let alpaca = AlpacaClient {
            base_url: mock_base_url.to_string(),
            data_url: mock_data_url.to_string(),
            headers,
            client,
            info: mock_account_response,
        };

        alpaca
    }

    #[tokio::test]
    async fn test_validate_keys() {
        // Valid keys
        assert!(AlpacaClient::validate_keys("PKTEST12345ABCDEFGHI", "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFG"));
        assert!(AlpacaClient::validate_keys("AKTEST12345ABCDEFGHI", "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFG"));

        // Invalid keys
        assert!(!AlpacaClient::validate_keys("INVALID", "secret"));
        assert!(!AlpacaClient::validate_keys("PKSHORT", "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFG"));
        assert!(!AlpacaClient::validate_keys("PKTEST12345ABCDEFGHI", "tooshort"));
        assert!(!AlpacaClient::validate_keys("XXTEST12345ABCDEFGHI", "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFG"));
    }

    #[tokio::test]
    async fn test_connect_invalid_keys() {
        let result = AlpacaClient::connect("invalid", "keys").await;
        assert!(matches!(result, Err(AlpacaError::InvalidKeyFormat)));
    }

    #[tokio::test]
    async fn test_make_request_success() {
        let mock_server = MockServer::start().await;

        // Setup mock response
        Mock::given(method("GET"))
            .and(path("/test-endpoint"))
            .and(header("APCA-API-KEY-ID", "PKTEST12345ABCDEFGHI"))
            .and(header("APCA-API-SECRET-KEY", "abcdefghijklmnopqrstuvwxyz1234567890ABCDEFG"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(json!({"status": "success"})))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.make_request(
            Method::GET,
            "/test-endpoint",
            &client.base_url,
            None::<&()>,
            None::<&()>,
            Some(std::time::Duration::from_secs(5)),
        ).await;

        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json, json!({"status": "success"}));
    }

    #[tokio::test]
    async fn test_make_request_http_error() {
        let mock_server = MockServer::start().await;

        // Setup error response
        Mock::given(method("GET"))
            .and(path("/error-endpoint"))
            .respond_with(ResponseTemplate::new(400)
                .set_body_string("Bad request"))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.make_request(
            Method::GET,
            "/error-endpoint",
            &client.base_url,
            None::<&()>,
            None::<&()>,
            None,
        ).await;

        assert!(result.is_err());
        match result {
            Err(AlpacaError::HttpError { status, message }) => {
                assert_eq!(status, StatusCode::BAD_REQUEST);
                assert_eq!(message, "Bad request");
            },
            _ => panic!("Expected HttpError but got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_make_request_with_body_and_query() {
        let mock_server = MockServer::start().await;

        // Setup mock response that validates body and query
        Mock::given(method("POST"))
            .and(path("/test-with-params"))
            .and(query_param("param1", "value1"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(json!({"received": true})))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        #[derive(Serialize)]
        struct TestQuery {
            param1: String,
        }

        #[derive(Serialize)]
        struct TestBody {
            data: String,
        }

        let query = TestQuery { param1: "value1".to_string() };
        let body = TestBody { data: "test-data".to_string() };

        let result = client.make_request(
            Method::POST,
            "/test-with-params",
            &client.base_url,
            Some(&query),
            Some(&body),
            None,
        ).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_account() {
        let mock_server = MockServer::start().await;

        // Setup mock response
        let account_data = json!({
            "id": "test-account-id",
            "account_number": "TEST123456",
            "status": "ACTIVE",
            "currency": "USD",
            "buying_power": "500000.00",
            "cash": "250000.00"
        });

        Mock::given(method("GET"))
            .and(path("/v2/account"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(account_data.clone()))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.get_account().await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), account_data);
    }

    #[tokio::test]
    async fn test_get_positions() {
        let mock_server = MockServer::start().await;

        // Setup mock response
        let positions_data = json!([
            {
                "asset_id": "asset-1",
                "symbol": "AAPL",
                "qty": "100",
                "side": "long",
                "market_value": "15000.00"
            },
            {
                "asset_id": "asset-2",
                "symbol": "MSFT",
                "qty": "50",
                "side": "long",
                "market_value": "12500.00"
            }
        ]);

        Mock::given(method("GET"))
            .and(path("/v2/positions"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(positions_data.clone()))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.get_positions().await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), positions_data);
    }

    #[tokio::test]
    async fn test_place_order() {
        let mock_server = MockServer::start().await;

        // Setup mock response
        let order_response = json!({
            "id": "order-id-123",
            "client_order_id": "client-order-id-123",
            "status": "new",
            "symbol": "AAPL",
            "qty": "10",
            "side": "buy",
            "type": "market",
            "time_in_force": "ioc"
        });

        Mock::given(method("POST"))
            .and(path("/v2/orders"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(order_response.clone()))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.place_order(
            "AAPL",
            10,
            "buy",
            None,
            None
        ).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), order_response);
    }

    #[tokio::test]
    async fn test_place_order_with_options() {
        let mock_server = MockServer::start().await;

        // Setup mock response
        let order_response = json!({
            "id": "order-id-124",
            "client_order_id": "client-order-id-124",
            "status": "new",
            "symbol": "TSLA",
            "qty": "5",
            "side": "sell",
            "type": "limit",
            "time_in_force": "day"
        });

        Mock::given(method("POST"))
            .and(path("/v2/orders"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(order_response.clone()))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.place_order(
            "TSLA",
            5,
            "sell",
            Some("limit"),
            Some("day")
        ).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), order_response);
    }

    #[tokio::test]
    async fn test_get_prices() {
        let mock_server = MockServer::start().await;

        // Setup mock response
        let prices_data = json!({
            "AAPL": {
                "t": "2023-05-01T12:00:00Z",
                "c": 150.25,
                "h": 152.00,
                "l": 149.50,
                "o": 151.00,
                "v": 5000000
            },
            "MSFT": {
                "t": "2023-05-01T12:00:00Z",
                "c": 280.75,
                "h": 282.50,
                "l": 279.00,
                "o": 281.25,
                "v": 3500000
            }
        });

        Mock::given(method("GET"))
            .and(path("/v2/stocks/bars/latest"))
            .and(query_param("symbols", "AAPL,MSFT"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(prices_data.clone()))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            "https://api.example.com",
            &mock_server.uri()
        ).await;

        let result = client.get_prices(
            &["AAPL", "MSFT"],
            "bars"
        ).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), prices_data);
    }

    #[tokio::test]
    async fn test_get_prices_invalid_type() {
        let client = create_test_client(
            "https://api.example.com",
            "https://data.example.com"
        ).await;

        let result = client.get_prices(
            &["AAPL", "MSFT"],
            "invalid_type"
        ).await;

        assert!(result.is_err());
        match result {
            Err(AlpacaError::Other(msg)) => {
                assert!(msg.contains("Invalid price type"));
            },
            _ => panic!("Expected Other error but got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_get_prices_empty_assets() {
        let client = create_test_client(
            "https://api.example.com",
            "https://data.example.com"
        ).await;

        let result = client.get_prices(
            &[],
            "bars"
        ).await;

        assert!(result.is_ok());
        let empty_obj = Value::Object(serde_json::Map::new());
        assert_eq!(result.unwrap(), empty_obj);
    }

    #[tokio::test]
    async fn test_get_order_info() {
        let mock_server = MockServer::start().await;

        // Setup mock response
        let order_data = json!({
            "id": "order-id-123",
            "client_order_id": "client-order-id-123",
            "status": "filled",
            "symbol": "AAPL",
            "qty": "10",
            "filled_qty": "10",
            "side": "buy",
            "type": "market",
            "time_in_force": "ioc"
        });

        Mock::given(method("GET"))
            .and(path("/v2/orders/order-id-123"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(order_data.clone()))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.get_order_info("order-id-123").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), order_data);
    }

    #[tokio::test]
    async fn test_error_handling_timeout() {
        let mock_server = MockServer::start().await;

        // Use delay to cause timeout
        Mock::given(method("GET"))
            .and(path("/slow-endpoint"))
            .respond_with(ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(2))) // Set a delay longer than our timeout
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        // Set a very short timeout to ensure it triggers
        let result = client.make_request(
            Method::GET,
            "/slow-endpoint",
            &client.base_url,
            None::<&()>,
            None::<&()>,
            Some(std::time::Duration::from_millis(100)), // Very short timeout
        ).await;

        // This should result in a timeout error
        assert!(matches!(result, Err(AlpacaError::Timeout)));
    }

    #[tokio::test]
    async fn test_rate_limit_error() {
        let mock_server = MockServer::start().await;

        // Setup rate limit error response
        Mock::given(method("GET"))
            .and(path("/v2/account"))
            .respond_with(ResponseTemplate::new(429)
                .set_body_string("Rate limit exceeded"))
            .mount(&mock_server)
            .await;

        let client = create_test_client(
            &mock_server.uri(),
            "https://data.example.com"
        ).await;

        let result = client.get_account().await;

        assert!(result.is_err());
        match result {
            Err(AlpacaError::HttpError { status, message }) => {
                assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
                assert_eq!(message, "Rate limit exceeded");
            },
            _ => panic!("Expected HttpError but got {:?}", result),
        }
    }
}

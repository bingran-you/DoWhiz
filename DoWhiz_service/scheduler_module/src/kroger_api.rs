//! Kroger API integration for grocery price comparison
//!
//! Kroger provides a public developer API at https://developer.kroger.com/
//! This module handles authentication and product/price queries.
//!
//! Required environment variables:
//! - KROGER_CLIENT_ID: OAuth2 client ID from developer.kroger.com
//! - KROGER_CLIENT_SECRET: OAuth2 client secret
//!
//! API documentation: https://developer.kroger.com/reference

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Kroger API client with automatic token refresh
pub struct KrogerClient {
    client: Client,
    client_id: String,
    client_secret: String,
    token: Arc<RwLock<Option<AccessToken>>>,
}

#[derive(Debug, Clone)]
struct AccessToken {
    access_token: String,
    expires_at: std::time::Instant,
}

/// Kroger product from search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerProduct {
    pub product_id: String,
    pub upc: Option<String>,
    pub description: String,
    pub brand: Option<String>,
    pub categories: Vec<String>,
    pub images: Vec<KrogerImage>,
    pub items: Vec<KrogerItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerImage {
    pub perspective: String,
    pub sizes: Vec<KrogerImageSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerImageSize {
    pub size: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerItem {
    pub item_id: String,
    pub size: Option<String>,
    pub price: Option<KrogerPrice>,
    pub fulfillment: KrogerFulfillment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerPrice {
    pub regular: f64,
    pub promo: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerFulfillment {
    pub curbside: bool,
    pub delivery: bool,
    pub in_store: bool,
    pub ship_to_home: bool,
}

/// Kroger store location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerStore {
    pub location_id: String,
    pub name: String,
    pub address: KrogerAddress,
    pub phone: Option<String>,
    pub hours: Option<KrogerHours>,
    pub distance_miles: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerAddress {
    pub address_line1: String,
    pub city: String,
    pub state: String,
    pub zip_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KrogerHours {
    pub open_24_hours: bool,
    pub monday: Option<String>,
    pub tuesday: Option<String>,
    pub wednesday: Option<String>,
    pub thursday: Option<String>,
    pub friday: Option<String>,
    pub saturday: Option<String>,
    pub sunday: Option<String>,
}

/// Search filters for Kroger products
#[derive(Debug, Clone, Default)]
pub struct KrogerSearchFilters {
    pub term: String,
    pub location_id: Option<String>,
    pub limit: Option<u32>,
    pub start: Option<u32>,
    pub fulfillment: Option<String>, // "ais" (in-store), "csp" (curbside), "dth" (delivery)
}

// API response structures (internal)
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    token_type: String,
}

#[derive(Debug, Deserialize)]
struct ProductSearchResponse {
    data: Vec<ProductData>,
    meta: Option<MetaData>,
}

#[derive(Debug, Deserialize)]
struct ProductData {
    #[serde(rename = "productId")]
    product_id: String,
    upc: Option<String>,
    description: Option<String>,
    brand: Option<String>,
    categories: Option<Vec<String>>,
    images: Option<Vec<ImageData>>,
    items: Option<Vec<ItemData>>,
}

#[derive(Debug, Deserialize)]
struct ImageData {
    perspective: Option<String>,
    sizes: Option<Vec<ImageSizeData>>,
}

#[derive(Debug, Deserialize)]
struct ImageSizeData {
    size: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ItemData {
    #[serde(rename = "itemId")]
    item_id: Option<String>,
    size: Option<String>,
    price: Option<PriceData>,
    fulfillment: Option<FulfillmentData>,
}

#[derive(Debug, Deserialize)]
struct PriceData {
    regular: Option<f64>,
    promo: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FulfillmentData {
    curbside: Option<bool>,
    delivery: Option<bool>,
    #[serde(rename = "inStore")]
    in_store: Option<bool>,
    #[serde(rename = "shipToHome")]
    ship_to_home: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MetaData {
    pagination: Option<PaginationData>,
}

#[derive(Debug, Deserialize)]
struct PaginationData {
    total: Option<u32>,
    start: Option<u32>,
    limit: Option<u32>,
}

/// Convert internal ProductData to public KrogerProduct
fn product_data_to_kroger_product(p: ProductData) -> KrogerProduct {
    KrogerProduct {
        product_id: p.product_id,
        upc: p.upc,
        description: p.description.unwrap_or_default(),
        brand: p.brand,
        categories: p.categories.unwrap_or_default(),
        images: p
            .images
            .unwrap_or_default()
            .into_iter()
            .map(|img| KrogerImage {
                perspective: img.perspective.unwrap_or_default(),
                sizes: img
                    .sizes
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| KrogerImageSize {
                        size: s.size.unwrap_or_default(),
                        url: s.url.unwrap_or_default(),
                    })
                    .collect(),
            })
            .collect(),
        items: p
            .items
            .unwrap_or_default()
            .into_iter()
            .map(|item| KrogerItem {
                item_id: item.item_id.unwrap_or_default(),
                size: item.size,
                price: item.price.map(|pr| KrogerPrice {
                    regular: pr.regular.unwrap_or(0.0),
                    promo: pr.promo,
                }),
                fulfillment: item
                    .fulfillment
                    .map(|f| KrogerFulfillment {
                        curbside: f.curbside.unwrap_or(false),
                        delivery: f.delivery.unwrap_or(false),
                        in_store: f.in_store.unwrap_or(false),
                        ship_to_home: f.ship_to_home.unwrap_or(false),
                    })
                    .unwrap_or(KrogerFulfillment {
                        curbside: false,
                        delivery: false,
                        in_store: false,
                        ship_to_home: false,
                    }),
            })
            .collect(),
    }
}

#[derive(Debug, Deserialize)]
struct LocationSearchResponse {
    data: Vec<LocationData>,
}

#[derive(Debug, Deserialize)]
struct LocationData {
    #[serde(rename = "locationId")]
    location_id: String,
    name: Option<String>,
    address: Option<AddressData>,
    phone: Option<String>,
    hours: Option<HoursData>,
    #[serde(rename = "distanceMiles")]
    distance_miles: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AddressData {
    #[serde(rename = "addressLine1")]
    address_line1: Option<String>,
    city: Option<String>,
    state: Option<String>,
    #[serde(rename = "zipCode")]
    zip_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HoursData {
    #[serde(rename = "open24Hours")]
    open_24_hours: Option<bool>,
    monday: Option<HoursDetail>,
    tuesday: Option<HoursDetail>,
    wednesday: Option<HoursDetail>,
    thursday: Option<HoursDetail>,
    friday: Option<HoursDetail>,
    saturday: Option<HoursDetail>,
    sunday: Option<HoursDetail>,
}

#[derive(Debug, Deserialize)]
struct HoursDetail {
    open: Option<String>,
    close: Option<String>,
}

impl KrogerClient {
    const TOKEN_URL: &'static str = "https://api.kroger.com/v1/connect/oauth2/token";
    const PRODUCT_URL: &'static str = "https://api.kroger.com/v1/products";
    const LOCATION_URL: &'static str = "https://api.kroger.com/v1/locations";

    /// Create a new Kroger API client from environment variables
    pub fn from_env() -> Result<Self, KrogerError> {
        let client_id = std::env::var("KROGER_CLIENT_ID")
            .map_err(|_| KrogerError::MissingCredentials("KROGER_CLIENT_ID not set".into()))?;
        let client_secret = std::env::var("KROGER_CLIENT_SECRET")
            .map_err(|_| KrogerError::MissingCredentials("KROGER_CLIENT_SECRET not set".into()))?;

        Ok(Self::new(client_id, client_secret))
    }

    /// Create a new Kroger API client with explicit credentials
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client: Client::new(),
            client_id,
            client_secret,
            token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get a valid access token, refreshing if necessary
    async fn get_token(&self) -> Result<String, KrogerError> {
        // Check if we have a valid token
        {
            let token = self.token.read().await;
            if let Some(ref t) = *token {
                if t.expires_at > std::time::Instant::now() {
                    return Ok(t.access_token.clone());
                }
            }
        }

        // Need to refresh token
        debug!("Refreshing Kroger access token");
        let new_token = self.fetch_token().await?;

        let mut token = self.token.write().await;
        *token = Some(new_token.clone());

        Ok(new_token.access_token)
    }

    /// Fetch a new access token from Kroger OAuth2
    async fn fetch_token(&self) -> Result<AccessToken, KrogerError> {
        let response = self
            .client
            .post(Self::TOKEN_URL)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[
                ("grant_type", "client_credentials"),
                ("scope", "product.compact"),
            ])
            .send()
            .await
            .map_err(|e| KrogerError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".into());
            error!("Kroger token request failed: {} - {}", status, body);
            return Err(KrogerError::AuthenticationFailed(format!(
                "{}: {}",
                status, body
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| KrogerError::ParseError(e.to_string()))?;

        // Set expiry 60 seconds before actual expiry for safety margin
        let expires_at = std::time::Instant::now()
            + std::time::Duration::from_secs(token_response.expires_in.saturating_sub(60));

        info!(
            "Kroger token obtained, expires in {} seconds",
            token_response.expires_in
        );

        Ok(AccessToken {
            access_token: token_response.access_token,
            expires_at,
        })
    }

    /// Search for products by keyword
    pub async fn search_products(
        &self,
        filters: &KrogerSearchFilters,
    ) -> Result<Vec<KrogerProduct>, KrogerError> {
        let token = self.get_token().await?;

        let mut url = format!("{}?filter.term={}", Self::PRODUCT_URL, &filters.term);

        if let Some(ref loc) = filters.location_id {
            url.push_str(&format!("&filter.locationId={}", loc));
        }
        if let Some(limit) = filters.limit {
            url.push_str(&format!("&filter.limit={}", limit));
        }
        if let Some(start) = filters.start {
            url.push_str(&format!("&filter.start={}", start));
        }
        if let Some(ref fulfillment) = filters.fulfillment {
            url.push_str(&format!("&filter.fulfillment={}", fulfillment));
        }

        debug!("Kroger product search: {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| KrogerError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".into());
            warn!("Kroger product search failed: {} - {}", status, body);
            return Err(KrogerError::ApiError(format!("{}: {}", status, body)));
        }

        let search_response: ProductSearchResponse = response
            .json()
            .await
            .map_err(|e| KrogerError::ParseError(e.to_string()))?;

        let products = search_response
            .data
            .into_iter()
            .map(product_data_to_kroger_product)
            .collect();

        Ok(products)
    }

    /// Find nearby Kroger stores by zip code
    pub async fn find_stores(
        &self,
        zip_code: &str,
        radius_miles: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Vec<KrogerStore>, KrogerError> {
        let token = self.get_token().await?;

        let radius = radius_miles.unwrap_or(10);
        let limit = limit.unwrap_or(10);

        let url = format!(
            "{}?filter.zipCode.near={}&filter.radiusInMiles={}&filter.limit={}",
            Self::LOCATION_URL,
            zip_code,
            radius,
            limit
        );

        debug!("Kroger store search: {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| KrogerError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".into());
            warn!("Kroger store search failed: {} - {}", status, body);
            return Err(KrogerError::ApiError(format!("{}: {}", status, body)));
        }

        let search_response: LocationSearchResponse = response
            .json()
            .await
            .map_err(|e| KrogerError::ParseError(e.to_string()))?;

        let stores = search_response
            .data
            .into_iter()
            .map(|loc| {
                let hours = loc.hours.map(|h| {
                    let format_hours = |detail: Option<HoursDetail>| {
                        detail.map(|d| {
                            format!(
                                "{}-{}",
                                d.open.unwrap_or_default(),
                                d.close.unwrap_or_default()
                            )
                        })
                    };
                    KrogerHours {
                        open_24_hours: h.open_24_hours.unwrap_or(false),
                        monday: format_hours(h.monday),
                        tuesday: format_hours(h.tuesday),
                        wednesday: format_hours(h.wednesday),
                        thursday: format_hours(h.thursday),
                        friday: format_hours(h.friday),
                        saturday: format_hours(h.saturday),
                        sunday: format_hours(h.sunday),
                    }
                });

                KrogerStore {
                    location_id: loc.location_id,
                    name: loc.name.unwrap_or_else(|| "Kroger".into()),
                    address: loc
                        .address
                        .map(|a| KrogerAddress {
                            address_line1: a.address_line1.unwrap_or_default(),
                            city: a.city.unwrap_or_default(),
                            state: a.state.unwrap_or_default(),
                            zip_code: a.zip_code.unwrap_or_default(),
                        })
                        .unwrap_or(KrogerAddress {
                            address_line1: String::new(),
                            city: String::new(),
                            state: String::new(),
                            zip_code: String::new(),
                        }),
                    phone: loc.phone,
                    hours,
                    distance_miles: loc.distance_miles,
                }
            })
            .collect();

        Ok(stores)
    }

    /// Get product details by product ID
    pub async fn get_product(
        &self,
        product_id: &str,
        location_id: Option<&str>,
    ) -> Result<Option<KrogerProduct>, KrogerError> {
        let token = self.get_token().await?;

        let mut url = format!("{}/{}", Self::PRODUCT_URL, product_id);
        if let Some(loc) = location_id {
            url.push_str(&format!("?filter.locationId={}", loc));
        }

        debug!("Kroger product detail: {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| KrogerError::NetworkError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".into());
            return Err(KrogerError::ApiError(format!("{}: {}", status, body)));
        }

        // Single product response has different structure
        #[derive(Deserialize)]
        struct SingleProductResponse {
            data: ProductData,
        }

        let product_response: SingleProductResponse = response
            .json()
            .await
            .map_err(|e| KrogerError::ParseError(e.to_string()))?;

        Ok(Some(product_data_to_kroger_product(product_response.data)))
    }
}

/// Errors that can occur when using the Kroger API
#[derive(Debug, thiserror::Error)]
pub enum KrogerError {
    #[error("Missing credentials: {0}")]
    MissingCredentials(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_filters_default() {
        let filters = KrogerSearchFilters {
            term: "pork belly".into(),
            ..Default::default()
        };
        assert_eq!(filters.term, "pork belly");
        assert!(filters.location_id.is_none());
    }

    #[tokio::test]
    #[ignore] // Requires valid credentials
    async fn test_search_products() {
        let client = KrogerClient::from_env().expect("Missing Kroger credentials");

        let filters = KrogerSearchFilters {
            term: "pork belly".into(),
            limit: Some(5),
            ..Default::default()
        };

        let products = client.search_products(&filters).await;
        println!("Search results: {:?}", products);
    }

    #[tokio::test]
    #[ignore] // Requires valid credentials
    async fn test_find_stores() {
        let client = KrogerClient::from_env().expect("Missing Kroger credentials");

        let stores = client.find_stores("48109", Some(10), Some(5)).await;
        println!("Store results: {:?}", stores);
    }
}

//! Grocery price comparison data store.
//!
//! Stores product prices, user preferences, and store information for the
//! smart grocery comparison agent.

use chrono::{DateTime, Utc};
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::{FindOptions, IndexOptions, UpdateOptions};
use mongodb::sync::Collection;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::mongo_store::{create_client_from_env, database_from_env, ensure_index_compatible};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum GroceryStoreError {
    #[error("mongodb error: {0}")]
    Mongo(#[from] mongodb::error::Error),
    #[error("mongo config error: {0}")]
    MongoConfig(String),
    #[error("invalid data: {0}")]
    InvalidData(String),
}

// ============================================================================
// Data Models
// ============================================================================

/// A grocery product with normalized name and category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroceryProduct {
    pub product_id: String,
    /// Canonical English name (e.g., "pork belly")
    pub name_en: String,
    /// Chinese name if applicable (e.g., "五花肉")
    pub name_zh: Option<String>,
    /// Product category (e.g., "meat", "snack", "condiment")
    pub category: String,
    /// Brand if applicable
    pub brand: Option<String>,
    /// Alternative names/aliases for search
    pub aliases: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A price observation for a product at a specific store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceEntry {
    pub entry_id: String,
    pub product_id: String,
    pub store_id: String,
    /// Raw price as displayed (e.g., 3.99)
    pub price: f64,
    /// Unit as displayed (e.g., "lb", "kg", "ea", "pack")
    pub unit: String,
    /// Normalized price per kg for comparison
    pub price_per_kg: Option<f64>,
    /// Source of this price data
    pub source: PriceSource,
    /// When this price was observed
    pub observed_at: DateTime<Utc>,
    /// When this entry was created in DB
    pub created_at: DateTime<Utc>,
    /// Optional notes (e.g., "on sale", "member price")
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PriceSource {
    /// Fetched via official API (most reliable)
    Api,
    /// Scraped from website
    WebScrape,
    /// Reported by user (receipt scan or manual)
    UserReport,
    /// Extracted from promotional email
    PromoEmail,
    /// Imported from external dataset
    Import,
}

/// A grocery store with location and type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroceryStore {
    pub store_id: String,
    pub name: String,
    /// Store type: "asian_market", "wholesale", "mainstream", "online"
    pub store_type: String,
    /// Address if physical
    pub address: Option<String>,
    /// City
    pub city: Option<String>,
    /// State
    pub state: Option<String>,
    /// ZIP code
    pub zip_code: Option<String>,
    /// Latitude for distance calculation
    pub latitude: Option<f64>,
    /// Longitude for distance calculation
    pub longitude: Option<f64>,
    /// Whether this is an online-only store
    pub is_online: bool,
    /// Delivery available
    pub has_delivery: bool,
    /// Minimum order for free delivery
    pub free_delivery_min: Option<f64>,
    /// Requires membership (Costco, Sam's)
    pub requires_membership: bool,
    /// Website URL
    pub website: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User's grocery preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroceryPreferences {
    pub user_id: String,
    /// User's ZIP code for distance calculation
    pub zip_code: Option<String>,
    /// User's address for more precise distance
    pub address: Option<String>,
    /// Latitude
    pub latitude: Option<f64>,
    /// Longitude
    pub longitude: Option<f64>,
    /// Taste preferences to avoid (e.g., "American-style sweets")
    pub taste_avoid: Vec<String>,
    /// Taste preferences to prefer (e.g., "Chinese-style pastries")
    pub taste_prefer: Vec<String>,
    /// Dietary restrictions (e.g., "no pork", "vegetarian")
    pub dietary: Vec<String>,
    /// Whether user has a car
    pub has_car: bool,
    /// Maximum drive time in minutes
    pub max_drive_minutes: Option<i32>,
    /// Preferred stores (store_ids)
    pub preferred_stores: Vec<String>,
    /// Store memberships (e.g., "costco", "sams")
    pub memberships: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Store Implementation
// ============================================================================

#[derive(Debug, Clone)]
pub struct GroceryStore_ {
    products: Collection<Document>,
    prices: Collection<Document>,
    stores: Collection<Document>,
    preferences: Collection<Document>,
}

impl GroceryStore_ {
    pub fn new() -> Result<Self, GroceryStoreError> {
        let client =
            create_client_from_env().map_err(|err| GroceryStoreError::MongoConfig(err.to_string()))?;
        let db = database_from_env(&client);

        let products = db.collection::<Document>("grocery_products");
        let prices = db.collection::<Document>("grocery_prices");
        let stores = db.collection::<Document>("grocery_stores");
        let preferences = db.collection::<Document>("grocery_preferences");

        // Ensure indexes
        Self::ensure_indexes(&products, &prices, &stores, &preferences)?;

        Ok(Self {
            products,
            prices,
            stores,
            preferences,
        })
    }

    fn ensure_indexes(
        products: &Collection<Document>,
        prices: &Collection<Document>,
        stores: &Collection<Document>,
        preferences: &Collection<Document>,
    ) -> Result<(), GroceryStoreError> {
        // Products indexes
        ensure_index_compatible(
            products,
            IndexModel::builder()
                .keys(doc! { "product_id": 1 })
                .options(IndexOptions::builder().unique(Some(true)).build())
                .build(),
        )?;
        ensure_index_compatible(
            products,
            IndexModel::builder()
                .keys(doc! { "name_en": 1 })
                .build(),
        )?;
        ensure_index_compatible(
            products,
            IndexModel::builder()
                .keys(doc! { "name_zh": 1 })
                .build(),
        )?;
        ensure_index_compatible(
            products,
            IndexModel::builder()
                .keys(doc! { "category": 1 })
                .build(),
        )?;

        // Prices indexes
        ensure_index_compatible(
            prices,
            IndexModel::builder()
                .keys(doc! { "product_id": 1, "store_id": 1, "observed_at": -1 })
                .build(),
        )?;
        ensure_index_compatible(
            prices,
            IndexModel::builder()
                .keys(doc! { "store_id": 1, "observed_at": -1 })
                .build(),
        )?;

        // Stores indexes
        ensure_index_compatible(
            stores,
            IndexModel::builder()
                .keys(doc! { "store_id": 1 })
                .options(IndexOptions::builder().unique(Some(true)).build())
                .build(),
        )?;
        ensure_index_compatible(
            stores,
            IndexModel::builder()
                .keys(doc! { "zip_code": 1 })
                .build(),
        )?;

        // Preferences indexes
        ensure_index_compatible(
            preferences,
            IndexModel::builder()
                .keys(doc! { "user_id": 1 })
                .options(IndexOptions::builder().unique(Some(true)).build())
                .build(),
        )?;

        Ok(())
    }

    // ========================================================================
    // Product Operations
    // ========================================================================

    /// Get or create a product by name.
    pub fn get_or_create_product(
        &self,
        name_en: &str,
        name_zh: Option<&str>,
        category: &str,
    ) -> Result<GroceryProduct, GroceryStoreError> {
        let normalized = normalize_product_name(name_en);
        let filter = doc! { "name_en": &normalized };

        if let Some(existing) = self.products.find_one(filter.clone(), None)? {
            return document_to_product(existing);
        }

        let now = Utc::now();
        let product_id = uuid::Uuid::new_v4().to_string();
        let doc = doc! {
            "product_id": &product_id,
            "name_en": &normalized,
            "name_zh": name_zh,
            "category": category,
            "brand": Bson::Null,
            "aliases": Vec::<String>::new(),
            "created_at": BsonDateTime::from_chrono(now),
            "updated_at": BsonDateTime::from_chrono(now),
        };

        self.products.insert_one(doc, None)?;
        Ok(GroceryProduct {
            product_id,
            name_en: normalized,
            name_zh: name_zh.map(String::from),
            category: category.to_string(),
            brand: None,
            aliases: vec![],
            created_at: now,
            updated_at: now,
        })
    }

    /// Search products by name (English or Chinese).
    pub fn search_products(&self, query: &str, limit: i64) -> Result<Vec<GroceryProduct>, GroceryStoreError> {
        let normalized = normalize_product_name(query);
        let filter = doc! {
            "$or": [
                { "name_en": { "$regex": &normalized, "$options": "i" } },
                { "name_zh": { "$regex": query, "$options": "i" } },
                { "aliases": { "$elemMatch": { "$regex": &normalized, "$options": "i" } } },
            ]
        };

        let options = FindOptions::builder().limit(limit).build();
        let cursor = self.products.find(filter, options)?;

        let mut results = Vec::new();
        for doc in cursor {
            results.push(document_to_product(doc?)?);
        }
        Ok(results)
    }

    // ========================================================================
    // Price Operations
    // ========================================================================

    /// Record a price observation.
    pub fn record_price(
        &self,
        product_id: &str,
        store_id: &str,
        price: f64,
        unit: &str,
        source: PriceSource,
        notes: Option<&str>,
    ) -> Result<PriceEntry, GroceryStoreError> {
        let now = Utc::now();
        let entry_id = uuid::Uuid::new_v4().to_string();
        let price_per_kg = normalize_price_to_kg(price, unit);

        let doc = doc! {
            "entry_id": &entry_id,
            "product_id": product_id,
            "store_id": store_id,
            "price": price,
            "unit": unit,
            "price_per_kg": price_per_kg,
            "source": mongodb::bson::to_bson(&source).unwrap_or(Bson::String("unknown".to_string())),
            "observed_at": BsonDateTime::from_chrono(now),
            "created_at": BsonDateTime::from_chrono(now),
            "notes": notes,
        };

        self.prices.insert_one(doc, None)?;
        Ok(PriceEntry {
            entry_id,
            product_id: product_id.to_string(),
            store_id: store_id.to_string(),
            price,
            unit: unit.to_string(),
            price_per_kg,
            source,
            observed_at: now,
            created_at: now,
            notes: notes.map(String::from),
        })
    }

    /// Get latest prices for a product across all stores.
    pub fn get_latest_prices(&self, product_id: &str) -> Result<Vec<PriceEntry>, GroceryStoreError> {
        // Aggregate to get latest price per store
        let pipeline = vec![
            doc! { "$match": { "product_id": product_id } },
            doc! { "$sort": { "observed_at": -1 } },
            doc! {
                "$group": {
                    "_id": "$store_id",
                    "doc": { "$first": "$$ROOT" }
                }
            },
            doc! { "$replaceRoot": { "newRoot": "$doc" } },
        ];

        let cursor = self.prices.aggregate(pipeline, None)?;
        let mut results = Vec::new();
        for doc in cursor {
            results.push(document_to_price_entry(doc?)?);
        }
        Ok(results)
    }

    /// Get price history for a product at a specific store.
    pub fn get_price_history(
        &self,
        product_id: &str,
        store_id: &str,
        limit: i64,
    ) -> Result<Vec<PriceEntry>, GroceryStoreError> {
        let filter = doc! {
            "product_id": product_id,
            "store_id": store_id,
        };
        let options = FindOptions::builder()
            .sort(doc! { "observed_at": -1 })
            .limit(limit)
            .build();

        let cursor = self.prices.find(filter, options)?;
        let mut results = Vec::new();
        for doc in cursor {
            results.push(document_to_price_entry(doc?)?);
        }
        Ok(results)
    }

    // ========================================================================
    // Store Operations
    // ========================================================================

    /// Get or create a store.
    pub fn get_or_create_store(
        &self,
        name: &str,
        store_type: &str,
        is_online: bool,
    ) -> Result<GroceryStore, GroceryStoreError> {
        let normalized_name = name.trim().to_lowercase();
        let filter = doc! { "name": { "$regex": format!("^{}$", regex::escape(&normalized_name)), "$options": "i" } };

        if let Some(existing) = self.stores.find_one(filter, None)? {
            return document_to_store(existing);
        }

        let now = Utc::now();
        let store_id = uuid::Uuid::new_v4().to_string();
        let doc = doc! {
            "store_id": &store_id,
            "name": name,
            "store_type": store_type,
            "address": Bson::Null,
            "city": Bson::Null,
            "state": Bson::Null,
            "zip_code": Bson::Null,
            "latitude": Bson::Null,
            "longitude": Bson::Null,
            "is_online": is_online,
            "has_delivery": is_online,
            "free_delivery_min": Bson::Null,
            "requires_membership": false,
            "website": Bson::Null,
            "created_at": BsonDateTime::from_chrono(now),
            "updated_at": BsonDateTime::from_chrono(now),
        };

        self.stores.insert_one(doc, None)?;
        Ok(GroceryStore {
            store_id,
            name: name.to_string(),
            store_type: store_type.to_string(),
            address: None,
            city: None,
            state: None,
            zip_code: None,
            latitude: None,
            longitude: None,
            is_online,
            has_delivery: is_online,
            free_delivery_min: None,
            requires_membership: false,
            website: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// List all stores.
    pub fn list_stores(&self) -> Result<Vec<GroceryStore>, GroceryStoreError> {
        let cursor = self.stores.find(doc! {}, None)?;
        let mut results = Vec::new();
        for doc in cursor {
            results.push(document_to_store(doc?)?);
        }
        Ok(results)
    }

    /// Get store by ID.
    pub fn get_store(&self, store_id: &str) -> Result<Option<GroceryStore>, GroceryStoreError> {
        let filter = doc! { "store_id": store_id };
        match self.stores.find_one(filter, None)? {
            Some(doc) => Ok(Some(document_to_store(doc)?)),
            None => Ok(None),
        }
    }

    // ========================================================================
    // Preferences Operations
    // ========================================================================

    /// Get user preferences.
    pub fn get_preferences(&self, user_id: &str) -> Result<Option<GroceryPreferences>, GroceryStoreError> {
        let filter = doc! { "user_id": user_id };
        match self.preferences.find_one(filter, None)? {
            Some(doc) => Ok(Some(document_to_preferences(doc)?)),
            None => Ok(None),
        }
    }

    /// Update user preferences (upsert).
    pub fn update_preferences(&self, prefs: &GroceryPreferences) -> Result<(), GroceryStoreError> {
        let filter = doc! { "user_id": &prefs.user_id };
        let now = Utc::now();
        let update = doc! {
            "$set": {
                "zip_code": &prefs.zip_code,
                "address": &prefs.address,
                "latitude": prefs.latitude,
                "longitude": prefs.longitude,
                "taste_avoid": &prefs.taste_avoid,
                "taste_prefer": &prefs.taste_prefer,
                "dietary": &prefs.dietary,
                "has_car": prefs.has_car,
                "max_drive_minutes": prefs.max_drive_minutes,
                "preferred_stores": &prefs.preferred_stores,
                "memberships": &prefs.memberships,
                "updated_at": BsonDateTime::from_chrono(now),
            },
            "$setOnInsert": {
                "user_id": &prefs.user_id,
                "created_at": BsonDateTime::from_chrono(now),
            }
        };
        let options = UpdateOptions::builder().upsert(true).build();
        self.preferences.update_one(filter, update, options)?;
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn normalize_product_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Normalize price to per-kg for comparison.
/// Returns None if unit is not recognized.
fn normalize_price_to_kg(price: f64, unit: &str) -> Option<f64> {
    let unit_lower = unit.to_lowercase();
    match unit_lower.as_str() {
        "kg" => Some(price),
        "lb" | "lbs" => Some(price * 2.20462), // 1 kg = 2.20462 lb
        "oz" => Some(price * 35.274),          // 1 kg = 35.274 oz
        "g" => Some(price * 1000.0),           // 1 kg = 1000 g
        "100g" => Some(price * 10.0),
        "500g" => Some(price * 2.0),
        _ => None, // "ea", "pack", etc. can't be normalized without weight info
    }
}

fn document_to_product(doc: Document) -> Result<GroceryProduct, GroceryStoreError> {
    let product_id = doc
        .get_str("product_id")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing product_id: {e}")))?
        .to_string();
    let name_en = doc
        .get_str("name_en")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing name_en: {e}")))?
        .to_string();
    let name_zh = doc.get_str("name_zh").ok().map(String::from);
    let category = doc
        .get_str("category")
        .unwrap_or("unknown")
        .to_string();
    let brand = doc.get_str("brand").ok().map(String::from);
    let aliases = doc
        .get_array("aliases")
        .ok()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let created_at = bson_datetime_to_utc(&doc, "created_at")?;
    let updated_at = bson_datetime_to_utc(&doc, "updated_at")?;

    Ok(GroceryProduct {
        product_id,
        name_en,
        name_zh,
        category,
        brand,
        aliases,
        created_at,
        updated_at,
    })
}

fn document_to_price_entry(doc: Document) -> Result<PriceEntry, GroceryStoreError> {
    let entry_id = doc
        .get_str("entry_id")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing entry_id: {e}")))?
        .to_string();
    let product_id = doc
        .get_str("product_id")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing product_id: {e}")))?
        .to_string();
    let store_id = doc
        .get_str("store_id")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing store_id: {e}")))?
        .to_string();
    let price = doc.get_f64("price").unwrap_or(0.0);
    let unit = doc.get_str("unit").unwrap_or("ea").to_string();
    let price_per_kg = doc.get_f64("price_per_kg").ok();
    let source = doc
        .get_str("source")
        .ok()
        .and_then(|s| serde_json::from_str(&format!("\"{}\"", s)).ok())
        .unwrap_or(PriceSource::UserReport);
    let observed_at = bson_datetime_to_utc(&doc, "observed_at")?;
    let created_at = bson_datetime_to_utc(&doc, "created_at")?;
    let notes = doc.get_str("notes").ok().map(String::from);

    Ok(PriceEntry {
        entry_id,
        product_id,
        store_id,
        price,
        unit,
        price_per_kg,
        source,
        observed_at,
        created_at,
        notes,
    })
}

fn document_to_store(doc: Document) -> Result<GroceryStore, GroceryStoreError> {
    let store_id = doc
        .get_str("store_id")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing store_id: {e}")))?
        .to_string();
    let name = doc
        .get_str("name")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing name: {e}")))?
        .to_string();
    let store_type = doc.get_str("store_type").unwrap_or("unknown").to_string();
    let address = doc.get_str("address").ok().map(String::from);
    let city = doc.get_str("city").ok().map(String::from);
    let state = doc.get_str("state").ok().map(String::from);
    let zip_code = doc.get_str("zip_code").ok().map(String::from);
    let latitude = doc.get_f64("latitude").ok();
    let longitude = doc.get_f64("longitude").ok();
    let is_online = doc.get_bool("is_online").unwrap_or(false);
    let has_delivery = doc.get_bool("has_delivery").unwrap_or(false);
    let free_delivery_min = doc.get_f64("free_delivery_min").ok();
    let requires_membership = doc.get_bool("requires_membership").unwrap_or(false);
    let website = doc.get_str("website").ok().map(String::from);
    let created_at = bson_datetime_to_utc(&doc, "created_at")?;
    let updated_at = bson_datetime_to_utc(&doc, "updated_at")?;

    Ok(GroceryStore {
        store_id,
        name,
        store_type,
        address,
        city,
        state,
        zip_code,
        latitude,
        longitude,
        is_online,
        has_delivery,
        free_delivery_min,
        requires_membership,
        website,
        created_at,
        updated_at,
    })
}

fn document_to_preferences(doc: Document) -> Result<GroceryPreferences, GroceryStoreError> {
    let user_id = doc
        .get_str("user_id")
        .map_err(|e| GroceryStoreError::InvalidData(format!("missing user_id: {e}")))?
        .to_string();
    let zip_code = doc.get_str("zip_code").ok().map(String::from);
    let address = doc.get_str("address").ok().map(String::from);
    let latitude = doc.get_f64("latitude").ok();
    let longitude = doc.get_f64("longitude").ok();
    let taste_avoid = doc
        .get_array("taste_avoid")
        .ok()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let taste_prefer = doc
        .get_array("taste_prefer")
        .ok()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let dietary = doc
        .get_array("dietary")
        .ok()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let has_car = doc.get_bool("has_car").unwrap_or(true);
    let max_drive_minutes = doc.get_i32("max_drive_minutes").ok();
    let preferred_stores = doc
        .get_array("preferred_stores")
        .ok()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let memberships = doc
        .get_array("memberships")
        .ok()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let created_at = bson_datetime_to_utc(&doc, "created_at")?;
    let updated_at = bson_datetime_to_utc(&doc, "updated_at")?;

    Ok(GroceryPreferences {
        user_id,
        zip_code,
        address,
        latitude,
        longitude,
        taste_avoid,
        taste_prefer,
        dietary,
        has_car,
        max_drive_minutes,
        preferred_stores,
        memberships,
        created_at,
        updated_at,
    })
}

fn bson_datetime_to_utc(doc: &Document, key: &str) -> Result<DateTime<Utc>, GroceryStoreError> {
    match doc.get(key) {
        Some(Bson::DateTime(value)) => Ok(value.to_chrono()),
        Some(Bson::String(value)) => DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| GroceryStoreError::InvalidData(format!("invalid datetime {}: {}", key, e))),
        _ => Ok(Utc::now()), // Default to now if missing
    }
}

// ============================================================================
// Global Instance
// ============================================================================

static GROCERY_STORE: std::sync::OnceLock<Option<Arc<GroceryStore_>>> = std::sync::OnceLock::new();

/// Get or initialize the global GroceryStore (returns None if not configured).
pub fn get_global_grocery_store() -> Option<Arc<GroceryStore_>> {
    GROCERY_STORE
        .get_or_init(|| match GroceryStore_::new() {
            Ok(store) => {
                tracing::info!("GroceryStore initialized for price comparison");
                Some(Arc::new(store))
            }
            Err(e) => {
                tracing::warn!("Failed to initialize GroceryStore: {}", e);
                None
            }
        })
        .clone()
}

// ============================================================================
// Seed Data
// ============================================================================

/// Seed common stores for the Ann Arbor area.
pub fn seed_ann_arbor_stores(store: &GroceryStore_) -> Result<(), GroceryStoreError> {
    let stores = vec![
        ("168 Asian Mart", "asian_market", false, Some("32393 John R Rd"), Some("Madison Heights"), Some("MI"), Some("48071")),
        ("H Mart Troy", "asian_market", false, Some("2850 W Maple Rd"), Some("Troy"), Some("MI"), Some("48084")),
        ("Weee", "online", true, None, None, None, None),
        ("Yami", "online", true, None, None, None, None),
        ("Costco Ann Arbor", "wholesale", false, Some("2800 S State St"), Some("Ann Arbor"), Some("MI"), Some("48104")),
        ("Sam's Club Ypsilanti", "wholesale", false, Some("3737 Carpenter Rd"), Some("Ypsilanti"), Some("MI"), Some("48197")),
        ("Kroger", "mainstream", false, None, Some("Ann Arbor"), Some("MI"), None),
        ("Aldi", "mainstream", false, None, Some("Ann Arbor"), Some("MI"), None),
    ];

    for (name, store_type, is_online, _address, _city, _state, _zip) in stores {
        let result = store.get_or_create_store(name, store_type, is_online)?;
        tracing::debug!("Seeded store: {} ({})", result.name, result.store_id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_price_to_kg() {
        assert_eq!(normalize_price_to_kg(10.0, "kg"), Some(10.0));
        assert!((normalize_price_to_kg(10.0, "lb").unwrap() - 22.0462).abs() < 0.01);
        assert_eq!(normalize_price_to_kg(10.0, "ea"), None);
    }

    #[test]
    fn test_normalize_product_name() {
        assert_eq!(normalize_product_name("  Pork Belly  "), "pork belly");
        assert_eq!(normalize_product_name("五花肉"), "五花肉");
    }
}

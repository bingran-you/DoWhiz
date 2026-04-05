//! Grocery preferences API routes.
//!
//! Handles saving/loading user grocery preferences.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::grocery_store::{GroceryPreferences, GroceryStore_, GroceryStoreError};

/// State for grocery routes
#[derive(Clone)]
pub struct GroceryState {
    pub grocery_store: Arc<GroceryStore_>,
}

impl GroceryState {
    pub fn from_env() -> Option<Self> {
        match GroceryStore_::new() {
            Ok(store) => Some(Self {
                grocery_store: Arc::new(store),
            }),
            Err(e) => {
                warn!("Failed to initialize grocery store: {}", e);
                None
            }
        }
    }
}

/// Query params for getting preferences
#[derive(Debug, Deserialize)]
pub struct GetPreferencesQuery {
    pub user_id: Option<String>,
    pub account_id: Option<String>,
}

/// Response for getting preferences
#[derive(Debug, Serialize)]
pub struct GetPreferencesResponse {
    pub preferences: Option<PreferencesData>,
}

/// Request body for saving preferences
#[derive(Debug, Deserialize)]
pub struct SavePreferencesRequest {
    pub user_id: Option<String>,
    pub account_id: Option<String>,
    pub preferences: PreferencesData,
    #[serde(default)]
    pub subscribe_weekly: bool,
}

/// Preferences data from frontend questionnaire
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferencesData {
    // Profile
    pub cultural_background: Option<String>,
    pub zip_code: Option<String>,
    pub city: Option<String>,
    pub household_size: Option<String>,

    // Shopping habits
    pub transport: Option<String>,
    pub shopping_preference: Option<String>,
    pub memberships: Option<Vec<String>>,
    pub preferred_stores: Option<Vec<String>>,
    pub other_stores: Option<String>,
    pub other_membership: Option<String>,

    // Categories
    pub main_categories: Option<Vec<String>>,

    // Category preferences
    pub meat_type: Option<Vec<String>>,
    pub meat_processing: Option<String>,
    pub meat_quantity: Option<String>,
    pub snack_flavor: Option<Vec<String>>,
    pub snack_brands_like: Option<String>,
    pub snack_brands_avoid: Option<String>,
    pub vegetable_types: Option<String>,
    pub vegetable_organic: Option<String>,
    pub hotpot_frequency: Option<String>,
    pub hotpot_base: Option<Vec<String>>,
    pub hotpot_brands: Option<String>,

    // Taste profile
    pub sweetness: Option<i32>,
    pub spiciness: Option<i32>,
    pub saltiness: Option<String>,
    pub american_sweets_opinion: Option<String>,
    pub avoid_foods: Option<String>,
    pub dietary_restrictions: Option<Vec<String>>,
    pub other_dietary: Option<String>,

    // Budget
    pub budget_mindset: Option<String>,
    pub priority_order: Option<Vec<String>>,
}

/// Response for saving preferences
#[derive(Debug, Serialize)]
pub struct SavePreferencesResponse {
    pub success: bool,
    pub user_id: String,
    pub message: String,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Get user preferences
async fn get_preferences(
    State(state): State<GroceryState>,
    Query(query): Query<GetPreferencesQuery>,
) -> impl IntoResponse {
    let user_id = match query.user_id.or(query.account_id) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "user_id or account_id required".into(),
                }),
            )
                .into_response();
        }
    };

    match state.grocery_store.get_preferences(&user_id) {
        Ok(Some(prefs)) => {
            let data = grocery_prefs_to_data(&prefs);
            (StatusCode::OK, Json(GetPreferencesResponse { preferences: Some(data) })).into_response()
        }
        Ok(None) => {
            (StatusCode::OK, Json(GetPreferencesResponse { preferences: None })).into_response()
        }
        Err(e) => {
            error!("Failed to get preferences for {}: {}", user_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to load preferences".into(),
                }),
            )
                .into_response()
        }
    }
}

/// Save user preferences
async fn save_preferences(
    State(state): State<GroceryState>,
    Json(req): Json<SavePreferencesRequest>,
) -> impl IntoResponse {
    // Determine user_id
    let user_id = match req.user_id.or(req.account_id) {
        Some(id) => id,
        None => {
            // Generate a new user_id if none provided
            Uuid::new_v4().to_string()
        }
    };

    // Convert frontend data to GroceryPreferences
    let prefs = data_to_grocery_prefs(&user_id, &req.preferences);

    // Save to MongoDB
    if let Err(e) = state.grocery_store.update_preferences(&prefs) {
        error!("Failed to save preferences for {}: {}", user_id, e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to save preferences".into(),
            }),
        )
            .into_response();
    }

    info!("Saved grocery preferences for user {}", user_id);

    let message = if req.subscribe_weekly {
        "Preferences saved! Weekly recommendations will be sent via email. You can also ask questions anytime by emailing us.".to_string()
    } else {
        "Preferences saved! You can ask grocery questions anytime by emailing us.".to_string()
    };

    (
        StatusCode::OK,
        Json(SavePreferencesResponse {
            success: true,
            user_id,
            message,
        }),
    )
        .into_response()
}

/// Convert GroceryPreferences to PreferencesData
fn grocery_prefs_to_data(prefs: &GroceryPreferences) -> PreferencesData {
    PreferencesData {
        cultural_background: None,
        zip_code: prefs.zip_code.clone(),
        city: None,
        household_size: None,
        transport: if prefs.has_car {
            Some("car_15min".to_string())
        } else {
            Some("no_car".to_string())
        },
        shopping_preference: None,
        memberships: Some(prefs.memberships.clone()),
        preferred_stores: Some(prefs.preferred_stores.clone()),
        other_stores: None,
        other_membership: None,
        main_categories: None,
        meat_type: None,
        meat_processing: None,
        meat_quantity: None,
        snack_flavor: None,
        snack_brands_like: None,
        snack_brands_avoid: prefs.taste_avoid.first().cloned(),
        vegetable_types: None,
        vegetable_organic: None,
        hotpot_frequency: None,
        hotpot_base: None,
        hotpot_brands: None,
        sweetness: None,
        spiciness: None,
        saltiness: None,
        american_sweets_opinion: None,
        avoid_foods: Some(prefs.taste_avoid.join(", ")),
        dietary_restrictions: Some(prefs.dietary.clone()),
        other_dietary: None,
        budget_mindset: None,
        priority_order: None,
    }
}

/// Convert PreferencesData to GroceryPreferences
fn data_to_grocery_prefs(user_id: &str, data: &PreferencesData) -> GroceryPreferences {
    let now = Utc::now();

    let has_car = data.transport.as_ref()
        .map(|t| t.starts_with("car_"))
        .unwrap_or(false);

    let max_drive_minutes = data.transport.as_ref().and_then(|t| {
        match t.as_str() {
            "car_15min" => Some(15),
            "car_30min" => Some(30),
            "car_60min" => Some(60),
            _ => None,
        }
    });

    let mut taste_avoid = Vec::new();
    if let Some(ref avoid) = data.avoid_foods {
        if !avoid.is_empty() {
            taste_avoid.push(avoid.clone());
        }
    }
    if let Some(ref brands) = data.snack_brands_avoid {
        if !brands.is_empty() {
            taste_avoid.push(brands.clone());
        }
    }

    let mut taste_prefer = Vec::new();
    if let Some(ref brands) = data.snack_brands_like {
        if !brands.is_empty() {
            taste_prefer.push(brands.clone());
        }
    }

    let dietary = data.dietary_restrictions.clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d != "none")
        .collect();

    GroceryPreferences {
        user_id: user_id.to_string(),
        zip_code: data.zip_code.clone(),
        address: None,
        latitude: None,
        longitude: None,
        taste_avoid,
        taste_prefer,
        dietary,
        has_car,
        max_drive_minutes,
        preferred_stores: data.preferred_stores.clone().unwrap_or_default(),
        memberships: data.memberships.clone().unwrap_or_default(),
        created_at: now,
        updated_at: now,
    }
}

/// Create the grocery router
pub fn grocery_router(state: GroceryState) -> Router {
    Router::new()
        .route("/api/grocery/preferences", get(get_preferences).post(save_preferences))
        .with_state(state)
}

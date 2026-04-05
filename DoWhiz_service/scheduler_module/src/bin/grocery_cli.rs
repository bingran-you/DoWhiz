//! Grocery CLI for agent to query prices and user preferences.
//!
//! Usage:
//!   grocery_cli preferences get <user_id>
//!   grocery_cli kroger search <term> [--location <zip_code>] [--limit <n>]
//!   grocery_cli kroger stores <zip_code> [--radius <miles>]

use clap::{Parser, Subcommand};
use scheduler_module::grocery_store::GroceryStore_;
use scheduler_module::kroger_api::{KrogerClient, KrogerSearchFilters};

/// Get Kroger API client or exit with error
fn get_kroger_client() -> KrogerClient {
    KrogerClient::from_env().unwrap_or_else(|e| {
        eprintln!("Kroger API error: {}", e);
        eprintln!("Ensure KROGER_CLIENT_ID and KROGER_CLIENT_SECRET are set.");
        std::process::exit(1);
    })
}

#[derive(Parser)]
#[command(name = "grocery_cli")]
#[command(about = "Query grocery prices and user preferences")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get or manage user preferences
    Preferences {
        #[command(subcommand)]
        action: PreferencesAction,
    },
    /// Query Kroger API
    Kroger {
        #[command(subcommand)]
        action: KrogerAction,
    },
}

#[derive(Subcommand)]
enum PreferencesAction {
    /// Get user preferences
    Get {
        /// User ID or account ID
        user_id: String,
    },
}

#[derive(Subcommand)]
enum KrogerAction {
    /// Search for products
    Search {
        /// Search term (e.g., "pork belly")
        term: String,
        /// Store location ID or zip code
        #[arg(short, long)]
        location: Option<String>,
        /// Maximum results
        #[arg(short = 'n', long, default_value = "5")]
        limit: u32,
    },
    /// Find nearby stores
    Stores {
        /// ZIP code
        zip_code: String,
        /// Search radius in miles
        #[arg(short, long, default_value = "10")]
        radius: u32,
        /// Maximum results
        #[arg(short = 'n', long, default_value = "5")]
        limit: u32,
    },
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    match cli.command {
        Commands::Preferences { action } => match action {
            PreferencesAction::Get { user_id } => {
                get_preferences(&user_id);
            }
        },
        Commands::Kroger { action } => match action {
            KrogerAction::Search {
                term,
                location,
                limit,
            } => {
                search_kroger(&term, location.as_deref(), limit).await;
            }
            KrogerAction::Stores {
                zip_code,
                radius,
                limit,
            } => {
                find_kroger_stores(&zip_code, radius, limit).await;
            }
        },
    }
}

fn get_preferences(user_id: &str) {
    match GroceryStore_::new() {
        Ok(store) => match store.get_preferences(user_id) {
            Ok(Some(prefs)) => {
                println!("User Preferences for {}:", user_id);
                println!("  ZIP Code: {}", prefs.zip_code.as_deref().unwrap_or("N/A"));
                println!("  Has Car: {}", prefs.has_car);
                if let Some(mins) = prefs.max_drive_minutes {
                    println!("  Max Drive: {} minutes", mins);
                }
                if !prefs.preferred_stores.is_empty() {
                    println!("  Preferred Stores: {}", prefs.preferred_stores.join(", "));
                }
                if !prefs.memberships.is_empty() {
                    println!("  Memberships: {}", prefs.memberships.join(", "));
                }
                if !prefs.dietary.is_empty() {
                    println!("  Dietary Restrictions: {}", prefs.dietary.join(", "));
                }
                if !prefs.taste_avoid.is_empty() {
                    println!("  Taste Avoid: {}", prefs.taste_avoid.join(", "));
                }
                if !prefs.taste_prefer.is_empty() {
                    println!("  Taste Prefer: {}", prefs.taste_prefer.join(", "));
                }
            }
            Ok(None) => {
                println!("No preferences found for user: {}", user_id);
                println!("User can set preferences at: /grocery/onboarding?user_id={}", user_id);
            }
            Err(e) => {
                eprintln!("Error loading preferences: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error connecting to database: {}", e);
            std::process::exit(1);
        }
    }
}

async fn search_kroger(term: &str, location: Option<&str>, limit: u32) {
    let client = get_kroger_client();

    // If location looks like a zip code, find the nearest store first
    let location_id = if let Some(loc) = location {
        if loc.chars().all(|c| c.is_ascii_digit()) && loc.len() == 5 {
            // It's a zip code, find nearest store
            match client.find_stores(loc, Some(10), Some(1)).await {
                Ok(stores) if !stores.is_empty() => {
                    println!("Using store: {} ({})", stores[0].name, stores[0].location_id);
                    Some(stores[0].location_id.clone())
                }
                _ => {
                    eprintln!("Warning: No stores found near ZIP {}. Prices may not be available.", loc);
                    None
                }
            }
        } else {
            Some(loc.to_string())
        }
    } else {
        None
    };

    let filters = KrogerSearchFilters {
        term: term.to_string(),
        location_id,
        limit: Some(limit),
        ..Default::default()
    };

    match client.search_products(&filters).await {
        Ok(products) => {
            if products.is_empty() {
                println!("No products found for: {}", term);
                return;
            }
            println!("Kroger Products for \"{}\":\n", term);
            for (i, product) in products.iter().enumerate() {
                println!("{}. {}", i + 1, product.description);
                if let Some(ref brand) = product.brand {
                    println!("   Brand: {}", brand);
                }
                for item in &product.items {
                    if let Some(ref size) = item.size {
                        print!("   Size: {}", size);
                    }
                    if let Some(ref price) = item.price {
                        print!("   Price: ${:.2}", price.regular);
                        if let Some(promo) = price.promo {
                            print!(" (Sale: ${:.2})", promo);
                        }
                    }
                    println!();
                }
                println!();
            }
        }
        Err(e) => {
            eprintln!("Kroger search error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn find_kroger_stores(zip_code: &str, radius: u32, limit: u32) {
    let client = get_kroger_client();

    match client.find_stores(zip_code, Some(radius), Some(limit)).await {
        Ok(stores) => {
            if stores.is_empty() {
                println!("No Kroger stores found within {} miles of {}", radius, zip_code);
                return;
            }
            println!("Kroger Stores near {}:\n", zip_code);
            for (i, store) in stores.iter().enumerate() {
                println!("{}. {} (ID: {})", i + 1, store.name, store.location_id);
                println!(
                    "   {}, {}, {} {}",
                    store.address.address_line1,
                    store.address.city,
                    store.address.state,
                    store.address.zip_code
                );
                if let Some(dist) = store.distance_miles {
                    println!("   Distance: {:.1} miles", dist);
                }
                if let Some(ref phone) = store.phone {
                    println!("   Phone: {}", phone);
                }
                println!();
            }
        }
        Err(e) => {
            eprintln!("Kroger store search error: {}", e);
            std::process::exit(1);
        }
    }
}

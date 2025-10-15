use anyhow::{Context, Result};
use retail_surveillance::{
    api::{self, AppState},
    database::Database,
    pos_integration::{POSIntegration, POSEvent},
};
use std::sync::Arc;
use std::env;
use tokio::sync::mpsc;
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct Config {
    database_url: String,
    mqtt_host: String,
    mqtt_port: u16,
    api_port: u16,
    enable_pos: bool,
}

impl Config {
    fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://surveillance:secure_password@localhost:5432/retail_surveillance".to_string()),
            mqtt_host: env::var("MQTT_HOST").unwrap_or_else(|_| "localhost".to_string()),
            mqtt_port: env::var("MQTT_PORT")
                .unwrap_or_else(|_| "1883".to_string())
                .parse()
                .unwrap_or(1883),
            api_port: env::var("API_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            enable_pos: env::var("ENABLE_POS")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(true),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("═══════════════════════════════════════════════════════════════");
    info!("     Retail Surveillance System - Phase 4: Database Integration");
    info!("═══════════════════════════════════════════════════════════════");

    // Load configuration
    let config = Config::from_env();
    info!("Configuration loaded");

    // Connect to database
    info!("Connecting to PostgreSQL...");
    let db = Arc::new(Database::new(&config.database_url).await?);

    // Run migrations
    match db.run_migrations().await {
        Ok(_) => info!("✅ Database migrations completed"),
        Err(e) => {
            warn!("⚠️  Migration error (may be already applied): {}", e);
        }
    }

    // Verify database connection
    db.health_check().await.context("Database health check failed")?;
    info!("✅ Database connection verified");

    // Create channel for POS events
    let (event_tx, mut event_rx) = mpsc::channel::<POSEvent>(100);

    // Start POS integration if enabled
    if config.enable_pos {
        info!("Starting POS integration...");

        let pos = POSIntegration::new(
            &config.mqtt_host,
            config.mqtt_port,
            "retail_surveillance_phase4",
        ).await?;

        let event_tx_clone = event_tx.clone();

        // Spawn POS event handler
        tokio::spawn(async move {
            if let Err(e) = pos.run_with_callback(move |event| {
                let tx = event_tx_clone.clone();
                Box::pin(async move {
                    if let Err(e) = tx.send(event).await {
                        error!("Failed to send event to processor: {}", e);
                    }
                })
            }).await {
                error!("POS integration error: {}", e);
            }
        });

        info!("✅ POS integration started");
    } else {
        info!("POS integration disabled");
    }

    // Spawn database event processor
    let db_clone = db.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            process_pos_event(db_clone.clone(), event).await;
        }
    });

    // Start REST API
    info!("Starting REST API on port {}...", config.api_port);
    let app_state = AppState { db: db.clone() };

    let api_handle = tokio::spawn(async move {
        if let Err(e) = api::serve(app_state, config.api_port).await {
            error!("API server error: {}", e);
        }
    });

    info!("✅ System fully operational");
    info!("");
    info!("📊 Dashboard: http://localhost:{}/api/v1/stats/dashboard", config.api_port);
    info!("🔍 Health: http://localhost:{}/health", config.api_port);
    info!("📋 Events: http://localhost:{}/api/v1/events", config.api_port);
    info!("🚨 Alerts: http://localhost:{}/api/v1/alerts", config.api_port);
    info!("");
    info!("Press Ctrl+C to shutdown");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    // Cleanup would go here

    Ok(())
}

async fn process_pos_event(db: Arc<Database>, event: POSEvent) {
    info!("Processing POS event: {} - {}", event.event_type, event.order_id);

    // Insert event into database
    let event_id = match db.insert_pos_event(&event).await {
        Ok(id) => {
            info!("✅ Event stored in database: {}", id);
            id
        }
        Err(e) => {
            error!("Failed to store event: {}", e);
            return;
        }
    };

    // Calculate risk score (simplified version)
    let risk_score = calculate_risk_score(&event);

    // Create alert if high risk
    if risk_score >= 0.5 {
        let reason = format!(
            "{} event with risk score {:.2} - Staff: {}, Amount: ${:.2}",
            event.event_type,
            risk_score,
            event.staff_id,
            event.amount.unwrap_or(0.0)
        );

        match db.insert_risk_alert(event_id, risk_score, reason).await {
            Ok(alert_id) => {
                warn!("🚨 Risk alert created: {} (score: {:.2})", alert_id, risk_score);
            }
            Err(e) => {
                error!("Failed to create risk alert: {}", e);
            }
        }
    }

    // Update video correlation if available
    if let Some(timestamp) = event.timestamp {
        let start_time = timestamp - chrono::Duration::seconds(30);
        let end_time = timestamp + chrono::Duration::seconds(30);

        if let Err(e) = db.update_video_correlation(
            event_id,
            "camera_001",
            start_time,
            end_time,
            None,
        ).await {
            warn!("Failed to update video correlation: {}", e);
        }
    }
}

fn calculate_risk_score(event: &POSEvent) -> f32 {
    use retail_surveillance::pos_integration::POSEventType;

    let mut score = match event.event_type {
        POSEventType::VoidTransaction => 0.4,
        POSEventType::RefundIssued => 0.5,
        POSEventType::SuspiciousReturn => 0.7,
        POSEventType::NoSaleOpened => 0.6,
        POSEventType::DiscountApplied => 0.2,
        POSEventType::PriceOverride => 0.3,
        POSEventType::PaymentCleared => 0.0,
    };

    // Add modifiers
    if let Some(amount) = event.amount {
        if amount > 1000.0 {
            score += 0.2;
        }
    }

    if let Some(discount) = event.discount_percent {
        if discount > 30.0 {
            score += 0.3;
        }
    }

    score.min(1.0)
}
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, FromRow, Row};
use uuid::Uuid;
use std::sync::Arc;
use tracing::{info, warn, error};

#[derive(Debug, Clone)]
pub struct Database {
    pool: Arc<PgPool>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct POSEventRecord {
    pub id: Uuid,
    pub event_id: String,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub store_id: String,
    pub register_id: Option<String>,
    pub staff_id: String,
    pub order_id: String,
    pub ticket_no: String,
    pub amount: Option<f64>,
    pub discount_percent: Option<f32>,
    pub item_count: Option<i32>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct RiskAlert {
    pub id: Uuid,
    pub event_id: Uuid,
    pub risk_score: f32,
    pub alert_level: String,
    pub reason: String,
    pub video_timestamp: Option<DateTime<Utc>>,
    pub video_path: Option<String>,
    pub acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct StaffRiskProfile {
    pub staff_id: String,
    pub store_id: String,
    pub total_events: i32,
    pub suspicious_events: i32,
    pub total_voids: i32,
    pub total_refunds: i32,
    pub total_discounts: i32,
    pub avg_discount_percent: Option<f32>,
    pub risk_score: f32,
    pub last_event_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct DailyStats {
    pub date: chrono::NaiveDate,
    pub store_id: String,
    pub total_transactions: i32,
    pub total_amount: f64,
    pub total_voids: i32,
    pub total_refunds: i32,
    pub total_discounts: i32,
    pub total_alerts: i32,
    pub high_risk_alerts: i32,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        info!("Connecting to PostgreSQL database...");

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .min_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(database_url)
            .await
            .context("Failed to connect to database")?;

        info!("Database connection established");

        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations...");

        sqlx::migrate!("./migrations")
            .run(&*self.pool)
            .await
            .context("Failed to run migrations")?;

        info!("Database migrations completed");
        Ok(())
    }

    pub async fn insert_pos_event(&self, event: &crate::pos_integration::POSEvent) -> Result<Uuid> {
        let id = Uuid::new_v4();

        let metadata = serde_json::to_value(&event.metadata)
            .context("Failed to serialize metadata")?;

        let event_type_str = format!("{:?}", event.event_type);
        let event_id_str = event.event_id.to_string();

        sqlx::query(
            r#"
            INSERT INTO pos_events (
                id, event_id, event_type, timestamp, store_id, register_id,
                staff_id, order_id, ticket_no, amount, discount_percent,
                item_count, metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#
        )
        .bind(id)
        .bind(event_id_str)
        .bind(event_type_str)
        .bind(event.timestamp)
        .bind(&event.store_id)
        .bind(Some(&event.register_id))  // register_id is Option<String> in schema
        .bind(&event.staff_id)
        .bind(&event.order_id)
        .bind(&event.ticket_no)
        .bind(event.amount)
        .bind(event.discount_percent)
        .bind(event.items.len() as i32)
        .bind(metadata)
        .execute(&*self.pool)
        .await
        .context("Failed to insert POS event")?;

        Ok(id)
    }

    pub async fn insert_risk_alert(
        &self,
        event_id: Uuid,
        risk_score: f32,
        reason: String,
    ) -> Result<Uuid> {
        let alert_level = match risk_score {
            s if s >= 0.8 => "CRITICAL",
            s if s >= 0.6 => "HIGH",
            s if s >= 0.4 => "MEDIUM",
            _ => "LOW",
        }.to_string();

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO risk_alerts (event_id, risk_score, alert_level, reason)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#
        )
        .bind(event_id)
        .bind(risk_score)
        .bind(alert_level)
        .bind(reason)
        .fetch_one(&*self.pool)
        .await
        .context("Failed to insert risk alert")?;

        Ok(id)
    }

    pub async fn get_staff_risk_profile(&self, staff_id: &str) -> Result<Option<StaffRiskProfile>> {
        let profile = sqlx::query_as::<_, StaffRiskProfile>(
            r#"
            SELECT
                staff_id, store_id, total_events, suspicious_events,
                total_voids, total_refunds, total_discounts,
                avg_discount_percent, risk_score, last_event_at, updated_at
            FROM staff_risk_profiles
            WHERE staff_id = $1
            "#
        )
        .bind(staff_id)
        .fetch_optional(&*self.pool)
        .await
        .context("Failed to fetch staff risk profile")?;

        Ok(profile)
    }

    pub async fn get_recent_alerts(&self, limit: i64) -> Result<Vec<RiskAlert>> {
        let alerts = sqlx::query_as::<_, RiskAlert>(
            r#"
            SELECT
                id, event_id, risk_score, alert_level, reason,
                video_timestamp, video_path, acknowledged,
                acknowledged_by, acknowledged_at, notes, created_at
            FROM risk_alerts
            WHERE NOT acknowledged
            ORDER BY created_at DESC
            LIMIT $1
            "#
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .context("Failed to fetch recent alerts")?;

        Ok(alerts)
    }

    pub async fn acknowledge_alert(
        &self,
        alert_id: Uuid,
        acknowledged_by: &str,
        notes: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE risk_alerts
            SET
                acknowledged = true,
                acknowledged_by = $2,
                acknowledged_at = NOW(),
                notes = $3
            WHERE id = $1
            "#
        )
        .bind(alert_id)
        .bind(acknowledged_by)
        .bind(notes)
        .execute(&*self.pool)
        .await
        .context("Failed to acknowledge alert")?;

        Ok(())
    }

    pub async fn get_daily_stats(
        &self,
        store_id: &str,
        date: chrono::NaiveDate,
    ) -> Result<Option<DailyStats>> {
        let row = sqlx::query(
            r#"
            SELECT
                date, store_id, total_transactions,
                total_amount, total_voids, total_refunds,
                total_discounts, total_alerts, high_risk_alerts
            FROM daily_stats
            WHERE store_id = $1 AND date = $2
            "#
        )
        .bind(store_id)
        .bind(date)
        .fetch_optional(&*self.pool)
        .await
        .context("Failed to fetch daily stats")?;

        if let Some(row) = row {
            use sqlx::Row;
            Ok(Some(DailyStats {
                date: row.try_get("date")?,
                store_id: row.try_get("store_id")?,
                total_transactions: row.try_get("total_transactions")?,
                total_amount: row.try_get::<f64, _>("total_amount")?,
                total_voids: row.try_get("total_voids")?,
                total_refunds: row.try_get("total_refunds")?,
                total_discounts: row.try_get("total_discounts")?,
                total_alerts: row.try_get("total_alerts")?,
                high_risk_alerts: row.try_get("high_risk_alerts")?,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn search_events(
        &self,
        store_id: Option<&str>,
        staff_id: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<POSEventRecord>> {
        let mut query = String::from(
            "SELECT * FROM pos_events WHERE 1=1"
        );

        if store_id.is_some() {
            query.push_str(" AND store_id = $1");
        }
        if staff_id.is_some() {
            query.push_str(" AND staff_id = $2");
        }
        if start_time.is_some() {
            query.push_str(" AND timestamp >= $3");
        }
        if end_time.is_some() {
            query.push_str(" AND timestamp <= $4");
        }

        query.push_str(" ORDER BY timestamp DESC LIMIT $5");

        // This is simplified - in production you'd use proper query builder
        let events = sqlx::query_as::<_, POSEventRecord>(&query)
            .bind(store_id)
            .bind(staff_id)
            .bind(start_time)
            .bind(end_time)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
            .context("Failed to search events")?;

        Ok(events)
    }

    pub async fn update_video_correlation(
        &self,
        event_id: Uuid,
        camera_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        video_path: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO video_correlations (
                event_id, camera_id, start_timestamp, end_timestamp, video_file_path
            ) VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (event_id, camera_id) DO UPDATE
            SET
                start_timestamp = $3,
                end_timestamp = $4,
                video_file_path = COALESCE($5, video_correlations.video_file_path)
            "#
        )
        .bind(event_id)
        .bind(camera_id)
        .bind(start_time)
        .bind(end_time)
        .bind(video_path)
        .execute(&*self.pool)
        .await
        .context("Failed to update video correlation")?;

        Ok(())
    }

    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&*self.pool)
            .await
            .context("Database health check failed")?;

        Ok(())
    }
}
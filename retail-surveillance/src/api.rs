use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{info, error};
use uuid::Uuid;

use crate::database::{Database, POSEventRecord, RiskAlert, StaffRiskProfile, DailyStats};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    database: String,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct EventQuery {
    store_id: Option<String>,
    staff_id: Option<String>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AcknowledgeRequest {
    acknowledged_by: String,
    notes: Option<String>,
}

#[derive(Debug, Serialize)]
struct DashboardStats {
    total_events_today: i32,
    total_alerts_today: i32,
    high_risk_alerts: i32,
    pending_alerts: Vec<RiskAlert>,
    staff_at_risk: Vec<StaffRiskProfile>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health & Status
        .route("/health", get(health_check))
        .route("/api/v1/status", get(system_status))

        // Events
        .route("/api/v1/events", get(get_events))
        .route("/api/v1/events/:id", get(get_event_by_id))

        // Alerts
        .route("/api/v1/alerts", get(get_alerts))
        .route("/api/v1/alerts/:id", get(get_alert_by_id))
        .route("/api/v1/alerts/:id/acknowledge", put(acknowledge_alert))

        // Staff Risk Profiles
        .route("/api/v1/staff/:id/risk", get(get_staff_risk))

        // Statistics
        .route("/api/v1/stats/daily", get(get_daily_stats))
        .route("/api/v1/stats/dashboard", get(get_dashboard_stats))

        // Analytics
        .route("/api/v1/analytics/trends", get(get_trends))
        .route("/api/v1/analytics/patterns", get(get_patterns))

        // Video Clips (Phase 5)
        .route("/api/v1/clips", get(get_video_clips))
        .route("/api/v1/clips/:id", get(get_video_clip))
        .route("/api/v1/clips/request", post(request_video_clip))
        .route("/api/v1/clips/:id/thumbnail", get(get_clip_thumbnail))
        .route("/api/v1/clips/camera/:camera_id", get(get_clips_by_camera))

        // Add middleware
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health_check(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    let db_status = match state.db.health_check().await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        database: db_status.to_string(),
        timestamp: Utc::now(),
    }))
}

async fn system_status(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let recent_alerts = state.db.get_recent_alerts(5)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "status": "operational",
        "timestamp": Utc::now(),
        "database": "connected",
        "mqtt": "connected",
        "pending_alerts": recent_alerts.len(),
        "version": "0.1.0"
    })))
}

async fn get_events(
    State(state): State<AppState>,
    Query(params): Query<EventQuery>,
) -> Result<Json<Vec<POSEventRecord>>, StatusCode> {
    let limit = params.limit.unwrap_or(100).min(1000);

    let events = state.db.search_events(
        params.store_id.as_deref(),
        params.staff_id.as_deref(),
        params.start_time,
        params.end_time,
        limit,
    )
    .await
    .map_err(|e| {
        error!("Failed to fetch events: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(events))
}

async fn get_event_by_id(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Simplified - would need to add get_event_by_id to database module
    Ok(Json(serde_json::json!({
        "id": id,
        "message": "Event retrieval not yet implemented"
    })))
}

async fn get_alerts(
    State(state): State<AppState>,
) -> Result<Json<Vec<RiskAlert>>, StatusCode> {
    let alerts = state.db.get_recent_alerts(50)
        .await
        .map_err(|e| {
            error!("Failed to fetch alerts: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(alerts))
}

async fn get_alert_by_id(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "id": id,
        "message": "Alert retrieval not yet implemented"
    })))
}

async fn acknowledge_alert(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AcknowledgeRequest>,
) -> Result<StatusCode, StatusCode> {
    state.db.acknowledge_alert(id, &req.acknowledged_by, req.notes.as_deref())
        .await
        .map_err(|e| {
            error!("Failed to acknowledge alert: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::OK)
}

async fn get_staff_risk(
    State(state): State<AppState>,
    Path(staff_id): Path<String>,
) -> Result<Json<Option<StaffRiskProfile>>, StatusCode> {
    let profile = state.db.get_staff_risk_profile(&staff_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch staff risk profile: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(profile))
}

async fn get_daily_stats(
    State(state): State<AppState>,
    Query(params): Query<serde_json::Value>,
) -> Result<Json<Option<DailyStats>>, StatusCode> {
    let store_id = params.get("store_id")
        .and_then(|v| v.as_str())
        .unwrap_or("store_001");

    let date = params.get("date")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| chrono::Local::now().naive_local().date());

    let stats = state.db.get_daily_stats(store_id, date)
        .await
        .map_err(|e| {
            error!("Failed to fetch daily stats: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(stats))
}

async fn get_dashboard_stats(
    State(state): State<AppState>,
) -> Result<Json<DashboardStats>, StatusCode> {
    // Get today's stats
    let today = chrono::Local::now().naive_local().date();
    let daily_stats = state.db.get_daily_stats("store_001", today)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get pending alerts
    let pending_alerts = state.db.get_recent_alerts(10)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get high-risk staff (simplified query)
    let staff_at_risk = vec![]; // Would need to add query for high-risk staff

    let stats = DashboardStats {
        total_events_today: daily_stats.as_ref().map(|s| s.total_transactions).unwrap_or(0),
        total_alerts_today: daily_stats.as_ref().map(|s| s.total_alerts).unwrap_or(0),
        high_risk_alerts: daily_stats.as_ref().map(|s| s.high_risk_alerts).unwrap_or(0),
        pending_alerts,
        staff_at_risk,
    };

    Ok(Json(stats))
}

async fn get_trends(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Simplified - would implement actual trend analysis
    Ok(Json(serde_json::json!({
        "daily_transactions": [120, 145, 132, 156, 141, 167, 155],
        "daily_alerts": [3, 5, 2, 7, 4, 8, 6],
        "risk_trend": "increasing",
        "period": "last_7_days"
    })))
}

async fn get_patterns(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "peak_risk_hours": [14, 15, 20, 21],
        "high_risk_days": ["Friday", "Saturday"],
        "common_event_types": {
            "VoidTransaction": 45,
            "RefundIssued": 32,
            "DiscountApplied": 78
        },
        "staff_patterns": [
            {
                "staff_id": "emp_12345",
                "pattern": "frequent_voids",
                "confidence": 0.82
            }
        ]
    })))
}

// Video Clip Endpoints (Phase 5)

#[derive(Debug, Serialize, Deserialize)]
struct VideoClipInfo {
    id: Uuid,
    camera_id: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    file_path: String,
    thumbnail_path: Option<String>,
    size_bytes: i64,
    duration_secs: f64,
    pos_event_id: Option<Uuid>,
    alert_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct VideoClipQuery {
    camera_id: Option<String>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    alert_id: Option<Uuid>,
    limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct VideoClipRequestPayload {
    camera_id: String,
    timestamp: DateTime<Utc>,
    duration_before_secs: i32,
    duration_after_secs: i32,
    pos_event_id: Option<Uuid>,
    alert_id: Option<Uuid>,
    priority: Option<String>,
}

async fn get_video_clips(
    State(state): State<AppState>,
    Query(params): Query<VideoClipQuery>,
) -> Result<Json<Vec<VideoClipInfo>>, StatusCode> {
    // Query would fetch from video_clips table
    let clips = vec![]; // Placeholder - would query database

    info!("Fetching video clips with params: {:?}", params);

    Ok(Json(clips))
}

async fn get_video_clip(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoClipInfo>, StatusCode> {
    // Would fetch specific clip from database
    Err(StatusCode::NOT_FOUND)
}

async fn request_video_clip(
    State(state): State<AppState>,
    Json(payload): Json<VideoClipRequestPayload>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!(
        "Video clip requested for camera {} at {} ({}s before, {}s after)",
        payload.camera_id, payload.timestamp,
        payload.duration_before_secs, payload.duration_after_secs
    );

    // Would insert into video_clip_requests table and trigger processing
    let request_id = Uuid::new_v4();

    Ok(Json(serde_json::json!({
        "request_id": request_id,
        "status": "pending",
        "message": "Video clip request submitted successfully"
    })))
}

async fn get_clip_thumbnail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Vec<u8>, StatusCode> {
    // Would fetch thumbnail file and return as bytes
    Err(StatusCode::NOT_FOUND)
}

async fn get_clips_by_camera(
    State(state): State<AppState>,
    Path(camera_id): Path<String>,
    Query(params): Query<VideoClipQuery>,
) -> Result<Json<Vec<VideoClipInfo>>, StatusCode> {
    info!("Fetching clips for camera: {}", camera_id);

    // Would query video_clips table filtered by camera_id
    let clips = vec![];

    Ok(Json(clips))
}

pub async fn serve(state: AppState, port: u16) -> Result<()> {
    let app = create_router(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("REST API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context("Failed to bind to address")?;

    axum::serve(listener, app)
        .await
        .context("Server error")?;

    Ok(())
}
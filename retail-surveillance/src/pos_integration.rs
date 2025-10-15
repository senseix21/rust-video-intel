use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Timelike};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use rand;

/// POS event types that trigger video correlation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum POSEventType {
    DiscountApplied,
    VoidTransaction,
    PaymentCleared,
    RefundIssued,
    PriceOverride,
    QuantityChanged,
    HighValueTransaction,
    NoSaleOpened,
    CashDrawerOpened,
    SuspiciousReturn,
}

/// POS event received from MQTT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct POSEvent {
    pub event_id: Uuid,
    pub event_type: POSEventType,
    pub timestamp: DateTime<Utc>,
    pub store_id: String,
    pub register_id: String,
    pub staff_id: String,
    pub order_id: String,
    pub ticket_no: String,
    pub amount: Option<f64>,
    pub original_amount: Option<f64>,
    pub discount_percent: Option<f64>,
    pub items: Vec<POSItem>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Individual item in a POS transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct POSItem {
    pub sku: String,
    pub name: String,
    pub quantity: i32,
    pub unit_price: f64,
    pub total_price: f64,
    pub discount: Option<f64>,
}

/// Video clip correlation with POS event
#[derive(Debug, Clone)]
pub struct VideoCorrelation {
    pub event_id: Uuid,
    pub camera_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub video_path: Option<String>,
    pub detections: Vec<Detection>,
    pub risk_score: f32,
}

/// Detection during the correlated time window
#[derive(Debug, Clone)]
pub struct Detection {
    pub timestamp: DateTime<Utc>,
    pub person_count: u32,
    pub suspicious_behavior: bool,
}

/// Configuration for POS integration
#[derive(Debug, Clone)]
pub struct POSConfig {
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_client_id: String,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub topics: Vec<String>,
    pub correlation_window_secs: i64,
    pub high_value_threshold: f64,
    pub discount_threshold: f64,
}

impl Default for POSConfig {
    fn default() -> Self {
        Self {
            mqtt_host: "localhost".to_string(),
            mqtt_port: 1883,
            mqtt_client_id: format!("surveillance_{}", Uuid::new_v4()),
            mqtt_username: None,
            mqtt_password: None,
            topics: vec![
                "pos/events/+/discount".to_string(),
                "pos/events/+/void".to_string(),
                "pos/events/+/refund".to_string(),
                "pos/events/+/drawer".to_string(),
            ],
            correlation_window_secs: 60,  // ±60 seconds around event
            high_value_threshold: 1000.0,
            discount_threshold: 30.0,     // 30% discount triggers alert
        }
    }
}

/// Risk scoring for POS events
pub struct RiskAnalyzer {
    config: POSConfig,
}

impl RiskAnalyzer {
    pub fn new(config: POSConfig) -> Self {
        Self { config }
    }

    pub fn calculate_risk_score(&self, event: &POSEvent) -> f32 {
        let mut score: f32 = 0.0;

        // Base risk by event type
        score += match event.event_type {
            POSEventType::VoidTransaction => 0.4,
            POSEventType::RefundIssued => 0.5,
            POSEventType::PriceOverride => 0.3,
            POSEventType::NoSaleOpened => 0.6,
            POSEventType::CashDrawerOpened => 0.3,
            POSEventType::SuspiciousReturn => 0.7,
            POSEventType::DiscountApplied => 0.2,
            _ => 0.1,
        };

        // High value transaction
        if let Some(amount) = event.amount {
            if amount > self.config.high_value_threshold {
                score += 0.2;
            }
        }

        // Large discount
        if let Some(discount) = event.discount_percent {
            if discount > self.config.discount_threshold {
                score += 0.3;
            }
        }

        // Multiple voids/refunds from same staff (would need history)
        // This is simplified - in production, check against database
        if event.metadata.get("repeat_offender").is_some() {
            score += 0.3;
        }

        // Time-based risk (after hours, etc.)
        let hour = event.timestamp.hour();
        if hour < 6 || hour > 22 {
            score += 0.1;  // Outside normal hours
        }

        score.min(1.0)  // Cap at 1.0
    }

    pub fn should_alert(&self, event: &POSEvent) -> bool {
        let risk_score = self.calculate_risk_score(event);

        // Alert on high risk or specific event types
        risk_score > 0.6 || matches!(
            event.event_type,
            POSEventType::VoidTransaction |
            POSEventType::RefundIssued |
            POSEventType::SuspiciousReturn |
            POSEventType::NoSaleOpened
        )
    }
}

/// Main POS integration service
pub struct POSIntegration {
    client: AsyncClient,
    eventloop: EventLoop,
    config: POSConfig,
    events: Arc<RwLock<Vec<POSEvent>>>,
    risk_analyzer: RiskAnalyzer,
}

impl POSIntegration {
    pub async fn new(config: POSConfig) -> Result<Self> {
        let mut mqtt_options = MqttOptions::new(
            &config.mqtt_client_id,
            &config.mqtt_host,
            config.mqtt_port,
        );

        mqtt_options
            .set_keep_alive(Duration::from_secs(30))
            .set_clean_session(true);

        if let (Some(user), Some(pass)) = (&config.mqtt_username, &config.mqtt_password) {
            mqtt_options.set_credentials(user, pass);
        }

        let (client, eventloop) = AsyncClient::new(mqtt_options, 100);

        // Subscribe to configured topics
        for topic in &config.topics {
            client.subscribe(topic, QoS::AtLeastOnce).await
                .context(format!("Failed to subscribe to topic: {}", topic))?;
            info!("Subscribed to MQTT topic: {}", topic);
        }

        let risk_analyzer = RiskAnalyzer::new(config.clone());

        Ok(Self {
            client,
            eventloop,
            config,
            events: Arc::new(RwLock::new(Vec::new())),
            risk_analyzer,
        })
    }

    /// Run the MQTT event loop
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting POS integration service");
        info!("MQTT broker: {}:{}", self.config.mqtt_host, self.config.mqtt_port);

        loop {
            match self.eventloop.poll().await {
                Ok(event) => {
                    self.handle_mqtt_event(event).await?;
                }
                Err(e) => {
                    error!("MQTT connection error: {}", e);
                    // Attempt to reconnect
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// Handle incoming MQTT events
    async fn handle_mqtt_event(&self, event: Event) -> Result<()> {
        match event {
            Event::Incoming(packet) => match packet {
                Packet::Publish(publish) => {
                    self.handle_pos_message(&publish.topic, &publish.payload).await?;
                }
                Packet::ConnAck(_) => {
                    info!("Connected to MQTT broker");
                }
                Packet::SubAck(_) => {
                    debug!("Subscription confirmed");
                }
                _ => {}
            },
            Event::Outgoing(_) => {}
        }
        Ok(())
    }

    /// Parse and process POS event message
    async fn handle_pos_message(&self, topic: &str, payload: &[u8]) -> Result<()> {
        // Parse JSON payload
        let event: POSEvent = serde_json::from_slice(payload)
            .context("Failed to parse POS event JSON")?;

        info!(
            "Received POS event: {:?} | Order: {} | Ticket: {} | Staff: {}",
            event.event_type, event.order_id, event.ticket_no, event.staff_id
        );

        // Calculate risk score
        let risk_score = self.risk_analyzer.calculate_risk_score(&event);
        info!("Risk score: {:.2}", risk_score);

        // Check if alert needed
        if self.risk_analyzer.should_alert(&event) {
            self.trigger_alert(&event, risk_score).await?;
        }

        // Store event for correlation
        let mut events = self.events.write().await;
        events.push(event.clone());

        // Limit stored events to last 1000
        if events.len() > 1000 {
            events.drain(0..100);
        }

        // Request video correlation for this time window
        self.correlate_with_video(&event).await?;

        Ok(())
    }

    /// Trigger alert for suspicious activity
    async fn trigger_alert(&self, event: &POSEvent, risk_score: f32) -> Result<()> {
        warn!(
            "🚨 ALERT: Suspicious activity detected!
            Type: {:?}
            Order ID: {}
            Ticket: {}
            Staff: {}
            Amount: ${:.2}
            Risk Score: {:.2}",
            event.event_type,
            event.order_id,
            event.ticket_no,
            event.staff_id,
            event.amount.unwrap_or(0.0),
            risk_score
        );

        // In production: Send to alerting system (Slack, email, etc.)
        // self.send_alert_notification(event, risk_score).await?;

        Ok(())
    }

    /// Request video clips for the time window around POS event
    async fn correlate_with_video(&self, event: &POSEvent) -> Result<()> {
        let start_time = event.timestamp - chrono::Duration::seconds(self.config.correlation_window_secs);
        let end_time = event.timestamp + chrono::Duration::seconds(self.config.correlation_window_secs);

        info!(
            "Requesting video correlation for {} to {}",
            start_time.format("%H:%M:%S"),
            end_time.format("%H:%M:%S")
        );

        // In production: This would trigger video clip extraction
        // let correlation = VideoCorrelation {
        //     event_id: event.event_id,
        //     camera_id: "camera_01".to_string(),
        //     start_time,
        //     end_time,
        //     video_path: None,
        //     detections: vec![],
        //     risk_score: self.risk_analyzer.calculate_risk_score(event),
        // };

        Ok(())
    }

    /// Get recent events
    pub async fn get_recent_events(&self, limit: usize) -> Vec<POSEvent> {
        let events = self.events.read().await;
        let start = if events.len() > limit {
            events.len() - limit
        } else {
            0
        };
        events[start..].to_vec()
    }

    /// Get events within time range
    pub async fn get_events_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<POSEvent> {
        let events = self.events.read().await;
        events
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .cloned()
            .collect()
    }
}

/// Example POS event publisher (for testing)
pub struct POSSimulator {
    client: AsyncClient,
}

impl POSSimulator {
    pub async fn new(host: &str, port: u16) -> Result<Self> {
        let mut mqtt_options = MqttOptions::new(
            format!("pos_simulator_{}", Uuid::new_v4()),
            host,
            port,
        );

        mqtt_options
            .set_keep_alive(Duration::from_secs(30))
            .set_clean_session(true);

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

        // Start event loop in background
        tokio::spawn(async move {
            loop {
                if let Err(e) = eventloop.poll().await {
                    error!("Simulator MQTT error: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });

        Ok(Self { client })
    }

    pub async fn publish_test_event(&self, event_type: POSEventType) -> Result<()> {
        let event = POSEvent {
            event_id: Uuid::new_v4(),
            event_type: event_type.clone(),
            timestamp: Utc::now(),
            store_id: "store_001".to_string(),
            register_id: "reg_02".to_string(),
            staff_id: "emp_12345".to_string(),
            order_id: format!("ORD{}", rand::random::<u32>() % 100000),
            ticket_no: format!("T{}", rand::random::<u32>() % 10000),
            amount: Some(150.00),
            original_amount: Some(200.00),
            discount_percent: Some(25.0),
            items: vec![
                POSItem {
                    sku: "SKU123".to_string(),
                    name: "Product A".to_string(),
                    quantity: 2,
                    unit_price: 100.0,
                    total_price: 200.0,
                    discount: Some(50.0),
                },
            ],
            metadata: HashMap::new(),
        };

        let topic = match event_type {
            POSEventType::DiscountApplied => "pos/events/store_001/discount",
            POSEventType::VoidTransaction => "pos/events/store_001/void",
            POSEventType::RefundIssued => "pos/events/store_001/refund",
            _ => "pos/events/store_001/general",
        };

        let payload = serde_json::to_vec(&event)?;
        self.client.publish(topic, QoS::AtLeastOnce, false, payload).await?;

        info!("Published test event: {:?} to {}", event_type, topic);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_scoring() {
        let config = POSConfig::default();
        let analyzer = RiskAnalyzer::new(config);

        let mut event = POSEvent {
            event_id: Uuid::new_v4(),
            event_type: POSEventType::VoidTransaction,
            timestamp: Utc::now(),
            store_id: "test".to_string(),
            register_id: "reg1".to_string(),
            staff_id: "staff1".to_string(),
            order_id: "order1".to_string(),
            ticket_no: "ticket1".to_string(),
            amount: Some(1500.0),  // High value
            original_amount: None,
            discount_percent: Some(40.0),  // High discount
            items: vec![],
            metadata: HashMap::new(),
        };

        let score = analyzer.calculate_risk_score(&event);
        assert!(score > 0.5, "High risk transaction should have high score");

        event.event_type = POSEventType::PaymentCleared;
        event.amount = Some(50.0);
        event.discount_percent = None;
        let score = analyzer.calculate_risk_score(&event);
        assert!(score < 0.3, "Normal transaction should have low score");
    }
}
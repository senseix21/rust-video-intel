-- Initial schema for retail surveillance system
-- Phase 4: Database Integration

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- POS Events table
CREATE TABLE pos_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_id VARCHAR(255) UNIQUE NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    store_id VARCHAR(50) NOT NULL,
    register_id VARCHAR(50),
    staff_id VARCHAR(50) NOT NULL,
    order_id VARCHAR(50) NOT NULL,
    ticket_no VARCHAR(50) NOT NULL,
    amount DECIMAL(10, 2),
    discount_percent DECIMAL(5, 2),
    item_count INTEGER,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Indexes for common queries
    INDEX idx_pos_events_timestamp (timestamp),
    INDEX idx_pos_events_store_id (store_id),
    INDEX idx_pos_events_staff_id (staff_id),
    INDEX idx_pos_events_event_type (event_type),
    INDEX idx_pos_events_order_id (order_id)
);

-- Risk Alerts table
CREATE TABLE risk_alerts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_id UUID REFERENCES pos_events(id) ON DELETE CASCADE,
    risk_score DECIMAL(3, 2) NOT NULL CHECK (risk_score >= 0 AND risk_score <= 1),
    alert_level VARCHAR(20) NOT NULL, -- LOW, MEDIUM, HIGH, CRITICAL
    reason TEXT NOT NULL,
    video_timestamp TIMESTAMPTZ,
    video_path TEXT,
    acknowledged BOOLEAN DEFAULT FALSE,
    acknowledged_by VARCHAR(50),
    acknowledged_at TIMESTAMPTZ,
    notes TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Indexes
    INDEX idx_risk_alerts_event_id (event_id),
    INDEX idx_risk_alerts_risk_score (risk_score),
    INDEX idx_risk_alerts_alert_level (alert_level),
    INDEX idx_risk_alerts_acknowledged (acknowledged),
    INDEX idx_risk_alerts_created_at (created_at)
);

-- Staff Risk Profiles table (tracks patterns)
CREATE TABLE staff_risk_profiles (
    staff_id VARCHAR(50) PRIMARY KEY,
    store_id VARCHAR(50) NOT NULL,
    total_events INTEGER DEFAULT 0,
    suspicious_events INTEGER DEFAULT 0,
    total_voids INTEGER DEFAULT 0,
    total_refunds INTEGER DEFAULT 0,
    total_discounts INTEGER DEFAULT 0,
    avg_discount_percent DECIMAL(5, 2),
    risk_score DECIMAL(3, 2) DEFAULT 0.0,
    last_event_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    -- Indexes
    INDEX idx_staff_risk_profiles_store_id (store_id),
    INDEX idx_staff_risk_profiles_risk_score (risk_score)
);

-- Daily Statistics table
CREATE TABLE daily_stats (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    date DATE NOT NULL,
    store_id VARCHAR(50) NOT NULL,
    total_transactions INTEGER DEFAULT 0,
    total_amount DECIMAL(12, 2) DEFAULT 0,
    total_voids INTEGER DEFAULT 0,
    total_refunds INTEGER DEFAULT 0,
    total_discounts INTEGER DEFAULT 0,
    total_alerts INTEGER DEFAULT 0,
    high_risk_alerts INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Unique constraint for one entry per store per day
    UNIQUE(date, store_id),

    -- Indexes
    INDEX idx_daily_stats_date (date),
    INDEX idx_daily_stats_store_id (store_id)
);

-- Video Correlations table
CREATE TABLE video_correlations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_id UUID REFERENCES pos_events(id) ON DELETE CASCADE,
    camera_id VARCHAR(50) NOT NULL,
    start_timestamp TIMESTAMPTZ NOT NULL,
    end_timestamp TIMESTAMPTZ NOT NULL,
    video_file_path TEXT,
    frame_numbers INTEGER[],
    detection_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Indexes
    INDEX idx_video_correlations_event_id (event_id),
    INDEX idx_video_correlations_camera_id (camera_id),
    INDEX idx_video_correlations_timestamp (start_timestamp, end_timestamp)
);

-- Function to update staff risk profile
CREATE OR REPLACE FUNCTION update_staff_risk_profile()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO staff_risk_profiles (staff_id, store_id, total_events, last_event_at)
    VALUES (NEW.staff_id, NEW.store_id, 1, NEW.timestamp)
    ON CONFLICT (staff_id) DO UPDATE
    SET
        total_events = staff_risk_profiles.total_events + 1,
        total_voids = CASE
            WHEN NEW.event_type = 'VoidTransaction'
            THEN staff_risk_profiles.total_voids + 1
            ELSE staff_risk_profiles.total_voids
        END,
        total_refunds = CASE
            WHEN NEW.event_type = 'RefundIssued'
            THEN staff_risk_profiles.total_refunds + 1
            ELSE staff_risk_profiles.total_refunds
        END,
        total_discounts = CASE
            WHEN NEW.event_type = 'DiscountApplied'
            THEN staff_risk_profiles.total_discounts + 1
            ELSE staff_risk_profiles.total_discounts
        END,
        last_event_at = NEW.timestamp,
        updated_at = NOW();

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for updating staff risk profile
CREATE TRIGGER trigger_update_staff_risk_profile
AFTER INSERT ON pos_events
FOR EACH ROW
EXECUTE FUNCTION update_staff_risk_profile();

-- Function to update daily statistics
CREATE OR REPLACE FUNCTION update_daily_stats()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO daily_stats (date, store_id, total_transactions, total_amount)
    VALUES (DATE(NEW.timestamp), NEW.store_id, 1, COALESCE(NEW.amount, 0))
    ON CONFLICT (date, store_id) DO UPDATE
    SET
        total_transactions = daily_stats.total_transactions + 1,
        total_amount = daily_stats.total_amount + COALESCE(NEW.amount, 0),
        total_voids = CASE
            WHEN NEW.event_type = 'VoidTransaction'
            THEN daily_stats.total_voids + 1
            ELSE daily_stats.total_voids
        END,
        total_refunds = CASE
            WHEN NEW.event_type = 'RefundIssued'
            THEN daily_stats.total_refunds + 1
            ELSE daily_stats.total_refunds
        END,
        total_discounts = CASE
            WHEN NEW.event_type = 'DiscountApplied'
            THEN daily_stats.total_discounts + 1
            ELSE daily_stats.total_discounts
        END;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for updating daily statistics
CREATE TRIGGER trigger_update_daily_stats
AFTER INSERT ON pos_events
FOR EACH ROW
EXECUTE FUNCTION update_daily_stats();

-- Sample views for common queries
CREATE VIEW high_risk_events AS
SELECT
    pe.*,
    ra.risk_score,
    ra.alert_level,
    ra.reason,
    ra.acknowledged
FROM pos_events pe
JOIN risk_alerts ra ON pe.id = ra.event_id
WHERE ra.risk_score >= 0.7
ORDER BY pe.timestamp DESC;

CREATE VIEW staff_risk_summary AS
SELECT
    srp.*,
    COUNT(DISTINCT pe.id) as events_today,
    COUNT(DISTINCT ra.id) as alerts_today
FROM staff_risk_profiles srp
LEFT JOIN pos_events pe ON srp.staff_id = pe.staff_id
    AND DATE(pe.timestamp) = CURRENT_DATE
LEFT JOIN risk_alerts ra ON pe.id = ra.event_id
GROUP BY srp.staff_id;
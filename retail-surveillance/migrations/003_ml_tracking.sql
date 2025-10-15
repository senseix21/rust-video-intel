-- Phase 6: ML People Detection and Tracking Schema
-- Adds support for storing ML detection results, people tracking, and zone analytics

-- People detections table
CREATE TABLE IF NOT EXISTS people_detections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    camera_id VARCHAR(50) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    frame_number BIGINT,
    detection_count INTEGER NOT NULL DEFAULT 0,
    detections JSONB NOT NULL DEFAULT '[]',
    confidence_avg REAL,
    processing_time_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Indexes for time-based queries
    CONSTRAINT detections_valid_count CHECK (detection_count >= 0)
);

CREATE INDEX idx_detections_camera_time ON people_detections (camera_id, timestamp DESC);
CREATE INDEX idx_detections_timestamp ON people_detections (timestamp DESC);
CREATE INDEX idx_detections_count ON people_detections (detection_count) WHERE detection_count > 0;

-- People tracking table
CREATE TABLE IF NOT EXISTS people_tracks (
    track_id INTEGER NOT NULL,
    camera_id VARCHAR(50) NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ NOT NULL,
    duration_seconds REAL GENERATED ALWAYS AS (
        EXTRACT(EPOCH FROM (last_seen - first_seen))
    ) STORED,
    bbox_history JSONB DEFAULT '[]',
    velocity JSONB,
    total_distance REAL,
    status VARCHAR(20) DEFAULT 'active',
    metadata JSONB DEFAULT '{}',

    PRIMARY KEY (track_id, camera_id),
    CONSTRAINT track_valid_times CHECK (last_seen >= first_seen),
    CONSTRAINT track_valid_status CHECK (status IN ('active', 'lost', 'completed'))
);

CREATE INDEX idx_tracks_camera ON people_tracks (camera_id);
CREATE INDEX idx_tracks_active ON people_tracks (camera_id, status) WHERE status = 'active';
CREATE INDEX idx_tracks_time_range ON people_tracks (first_seen, last_seen);

-- Zone definitions table
CREATE TABLE IF NOT EXISTS zones (
    zone_id VARCHAR(50) PRIMARY KEY,
    camera_id VARCHAR(50) NOT NULL,
    zone_name VARCHAR(100) NOT NULL,
    zone_type VARCHAR(50) DEFAULT 'counting',
    polygon JSONB NOT NULL,
    enabled BOOLEAN DEFAULT true,
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT zone_valid_type CHECK (zone_type IN ('counting', 'entrance', 'exit', 'restricted', 'interest'))
);

CREATE INDEX idx_zones_camera ON zones (camera_id);
CREATE INDEX idx_zones_enabled ON zones (enabled) WHERE enabled = true;

-- Zone analytics table
CREATE TABLE IF NOT EXISTS zone_analytics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    zone_id VARCHAR(50) REFERENCES zones(zone_id) ON DELETE CASCADE,
    timestamp TIMESTAMPTZ NOT NULL,
    entry_count INTEGER DEFAULT 0,
    exit_count INTEGER DEFAULT 0,
    current_occupancy INTEGER DEFAULT 0,
    dwell_time_avg REAL,
    track_ids INTEGER[] DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT analytics_valid_counts CHECK (
        entry_count >= 0 AND
        exit_count >= 0 AND
        current_occupancy >= 0
    )
);

CREATE INDEX idx_zone_analytics_zone_time ON zone_analytics (zone_id, timestamp DESC);
CREATE INDEX idx_zone_analytics_timestamp ON zone_analytics (timestamp DESC);

-- Hourly people count statistics
CREATE TABLE IF NOT EXISTS hourly_people_stats (
    camera_id VARCHAR(50) NOT NULL,
    hour_bucket TIMESTAMPTZ NOT NULL,
    total_detections INTEGER DEFAULT 0,
    unique_tracks INTEGER DEFAULT 0,
    avg_count_per_frame REAL,
    max_count INTEGER DEFAULT 0,
    zone_entries INTEGER DEFAULT 0,
    zone_exits INTEGER DEFAULT 0,

    PRIMARY KEY (camera_id, hour_bucket)
);

CREATE INDEX idx_hourly_stats_time ON hourly_people_stats (hour_bucket DESC);

-- ML model metrics table
CREATE TABLE IF NOT EXISTS ml_metrics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    model_name VARCHAR(100),
    model_version VARCHAR(50),
    inference_count INTEGER DEFAULT 0,
    avg_inference_time_ms REAL,
    avg_detections_per_frame REAL,
    total_people_detected BIGINT DEFAULT 0,
    error_count INTEGER DEFAULT 0,
    uptime_seconds INTEGER
);

-- Function to update zone occupancy
CREATE OR REPLACE FUNCTION update_zone_occupancy()
RETURNS TRIGGER AS $$
BEGIN
    -- Calculate current occupancy as entries - exits
    NEW.current_occupancy = GREATEST(0, NEW.entry_count - NEW.exit_count);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_zone_occupancy
BEFORE INSERT OR UPDATE ON zone_analytics
FOR EACH ROW
EXECUTE FUNCTION update_zone_occupancy();

-- Function to aggregate hourly statistics
CREATE OR REPLACE FUNCTION aggregate_hourly_stats()
RETURNS void AS $$
BEGIN
    INSERT INTO hourly_people_stats (
        camera_id,
        hour_bucket,
        total_detections,
        unique_tracks,
        avg_count_per_frame,
        max_count
    )
    SELECT
        camera_id,
        date_trunc('hour', timestamp) as hour_bucket,
        SUM(detection_count) as total_detections,
        COUNT(DISTINCT jsonb_array_elements(detections)->>'track_id') as unique_tracks,
        AVG(detection_count) as avg_count_per_frame,
        MAX(detection_count) as max_count
    FROM people_detections
    WHERE timestamp >= NOW() - INTERVAL '1 hour'
    GROUP BY camera_id, date_trunc('hour', timestamp)
    ON CONFLICT (camera_id, hour_bucket) DO UPDATE
    SET
        total_detections = EXCLUDED.total_detections,
        unique_tracks = EXCLUDED.unique_tracks,
        avg_count_per_frame = EXCLUDED.avg_count_per_frame,
        max_count = EXCLUDED.max_count;
END;
$$ LANGUAGE plpgsql;

-- View for real-time people count
CREATE VIEW current_people_count AS
SELECT
    pd.camera_id,
    pd.timestamp as last_update,
    pd.detection_count as current_count,
    COUNT(DISTINCT pt.track_id) as active_tracks,
    AVG(pd.confidence_avg) as avg_confidence
FROM (
    SELECT DISTINCT ON (camera_id) *
    FROM people_detections
    ORDER BY camera_id, timestamp DESC
) pd
LEFT JOIN people_tracks pt ON pd.camera_id = pt.camera_id AND pt.status = 'active'
GROUP BY pd.camera_id, pd.timestamp, pd.detection_count;

-- View for zone occupancy
CREATE VIEW zone_occupancy_view AS
SELECT
    z.zone_id,
    z.zone_name,
    z.camera_id,
    za.current_occupancy,
    za.entry_count,
    za.exit_count,
    za.timestamp as last_update
FROM zones z
LEFT JOIN LATERAL (
    SELECT *
    FROM zone_analytics
    WHERE zone_id = z.zone_id
    ORDER BY timestamp DESC
    LIMIT 1
) za ON true
WHERE z.enabled = true;

-- View for tracking analytics
CREATE VIEW tracking_analytics AS
SELECT
    camera_id,
    DATE(first_seen) as date,
    COUNT(*) as total_tracks,
    AVG(duration_seconds) as avg_duration,
    MAX(duration_seconds) as max_duration,
    AVG(total_distance) as avg_distance
FROM people_tracks
WHERE status = 'completed'
GROUP BY camera_id, DATE(first_seen);

-- Function to clean old ML data
CREATE OR REPLACE FUNCTION cleanup_old_ml_data()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    -- Delete detections older than 7 days
    DELETE FROM people_detections
    WHERE timestamp < NOW() - INTERVAL '7 days';

    GET DIAGNOSTICS deleted_count = ROW_COUNT;

    -- Delete completed tracks older than 30 days
    DELETE FROM people_tracks
    WHERE status = 'completed' AND last_seen < NOW() - INTERVAL '30 days';

    -- Delete old zone analytics
    DELETE FROM zone_analytics
    WHERE timestamp < NOW() - INTERVAL '30 days';

    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Indexes for performance
CREATE INDEX idx_detections_recent ON people_detections (timestamp DESC)
WHERE timestamp > NOW() - INTERVAL '1 day';

CREATE INDEX idx_tracks_recent ON people_tracks (last_seen DESC)
WHERE last_seen > NOW() - INTERVAL '1 day';

-- Grant permissions (adjust as needed)
-- GRANT SELECT ON ALL TABLES IN SCHEMA public TO surveillance_reader;
-- GRANT ALL ON ALL TABLES IN SCHEMA public TO surveillance_admin;
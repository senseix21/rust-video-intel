-- Phase 5: Video Clip Storage Schema
-- Adds support for storing video clips and thumbnails linked to POS events and alerts

-- Video clips table
CREATE TABLE IF NOT EXISTS video_clips (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    camera_id VARCHAR(50) NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    file_path TEXT NOT NULL,
    thumbnail_path TEXT,
    size_bytes BIGINT NOT NULL,
    duration_secs REAL NOT NULL,
    pos_event_id UUID REFERENCES pos_events(id) ON DELETE CASCADE,
    alert_id UUID REFERENCES risk_alerts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}',

    CONSTRAINT video_clips_valid_times CHECK (end_time > start_time),
    CONSTRAINT video_clips_valid_duration CHECK (duration_secs > 0)
);

-- Indexes for video clips
CREATE INDEX idx_video_clips_camera_time ON video_clips (camera_id, start_time);
CREATE INDEX idx_video_clips_pos_event ON video_clips (pos_event_id) WHERE pos_event_id IS NOT NULL;
CREATE INDEX idx_video_clips_alert ON video_clips (alert_id) WHERE alert_id IS NOT NULL;
CREATE INDEX idx_video_clips_created ON video_clips (created_at);

-- Camera configuration table
CREATE TABLE IF NOT EXISTS cameras (
    camera_id VARCHAR(50) PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    location VARCHAR(200),
    rtsp_url TEXT NOT NULL,
    enabled BOOLEAN DEFAULT true,
    buffer_duration_secs INTEGER DEFAULT 120,
    retention_days INTEGER DEFAULT 30,
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Video clip requests queue
CREATE TABLE IF NOT EXISTS video_clip_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    camera_id VARCHAR(50) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    duration_before_secs INTEGER NOT NULL DEFAULT 30,
    duration_after_secs INTEGER NOT NULL DEFAULT 30,
    pos_event_id UUID REFERENCES pos_events(id),
    alert_id UUID REFERENCES risk_alerts(id),
    priority VARCHAR(20) DEFAULT 'medium',
    status VARCHAR(20) DEFAULT 'pending',
    clip_id UUID REFERENCES video_clips(id),
    error_message TEXT,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ,

    CONSTRAINT clip_request_valid_durations CHECK (
        duration_before_secs >= 0 AND
        duration_after_secs >= 0
    ),
    CONSTRAINT clip_request_valid_priority CHECK (
        priority IN ('low', 'medium', 'high', 'critical')
    ),
    CONSTRAINT clip_request_valid_status CHECK (
        status IN ('pending', 'processing', 'completed', 'failed')
    )
);

-- Index for pending requests
CREATE INDEX idx_clip_requests_pending ON video_clip_requests (priority DESC, requested_at)
WHERE status = 'pending';

-- Function to automatically create clip request when alert is triggered
CREATE OR REPLACE FUNCTION create_clip_request_on_alert()
RETURNS TRIGGER AS $$
BEGIN
    -- Only for high-risk alerts
    IF NEW.risk_score >= 0.4 THEN
        INSERT INTO video_clip_requests (
            camera_id,
            timestamp,
            duration_before_secs,
            duration_after_secs,
            alert_id,
            priority
        )
        SELECT
            COALESCE(pe.camera_id, 'camera_001'),
            NEW.triggered_at,
            30,  -- 30 seconds before
            30,  -- 30 seconds after
            NEW.id,
            CASE
                WHEN NEW.risk_score >= 0.8 THEN 'critical'
                WHEN NEW.risk_score >= 0.6 THEN 'high'
                ELSE 'medium'
            END
        FROM pos_events pe
        WHERE pe.id = NEW.pos_event_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-create clip requests
CREATE TRIGGER trigger_clip_on_alert
AFTER INSERT ON risk_alerts
FOR EACH ROW
EXECUTE FUNCTION create_clip_request_on_alert();

-- View for video clips with event details
CREATE VIEW video_clips_detailed AS
SELECT
    vc.*,
    pe.event_type as pos_event_type,
    pe.staff_id,
    pe.amount,
    ra.risk_score,
    ra.alert_type,
    c.name as camera_name,
    c.location as camera_location
FROM video_clips vc
LEFT JOIN pos_events pe ON vc.pos_event_id = pe.id
LEFT JOIN risk_alerts ra ON vc.alert_id = ra.id
LEFT JOIN cameras c ON vc.camera_id = c.camera_id;

-- Statistics view
CREATE VIEW video_storage_stats AS
SELECT
    camera_id,
    COUNT(*) as clip_count,
    SUM(size_bytes) as total_bytes,
    AVG(duration_secs) as avg_duration_secs,
    MIN(start_time) as earliest_clip,
    MAX(end_time) as latest_clip
FROM video_clips
GROUP BY camera_id;

-- Function to clean up old video clips
CREATE OR REPLACE FUNCTION cleanup_old_clips()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    WITH deleted AS (
        DELETE FROM video_clips
        WHERE created_at < NOW() - INTERVAL '30 days'
        RETURNING *
    )
    SELECT COUNT(*) INTO deleted_count FROM deleted;

    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Update trigger for cameras table
CREATE OR REPLACE FUNCTION update_camera_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_camera_updated_at
BEFORE UPDATE ON cameras
FOR EACH ROW
EXECUTE FUNCTION update_camera_timestamp();
-- Initialize database for Bitquery Kafka Consumer
-- This script sets up tables for offset management and monitoring

-- Create schema for kafka operations
CREATE SCHEMA IF NOT EXISTS kafka_ops;

-- Table for storing Kafka consumer offsets
CREATE TABLE IF NOT EXISTS kafka_ops.consumer_offsets (
    id SERIAL PRIMARY KEY,
    topic VARCHAR(255) NOT NULL,
    partition_id INTEGER NOT NULL,
    consumer_group VARCHAR(255) NOT NULL,
    offset_value BIGINT NOT NULL,
    committed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    
    UNIQUE(topic, partition_id, consumer_group)
);

-- Index for fast lookups
CREATE INDEX IF NOT EXISTS idx_consumer_offsets_lookup 
ON kafka_ops.consumer_offsets (topic, partition_id, consumer_group);

CREATE INDEX IF NOT EXISTS idx_consumer_offsets_committed_at 
ON kafka_ops.consumer_offsets (committed_at);

-- Table for storing processing metrics
CREATE TABLE IF NOT EXISTS kafka_ops.processing_metrics (
    id SERIAL PRIMARY KEY,
    instance_id VARCHAR(255) NOT NULL,
    metric_name VARCHAR(255) NOT NULL,
    metric_value DOUBLE PRECISION NOT NULL,
    labels JSONB,
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index for metrics queries
CREATE INDEX IF NOT EXISTS idx_processing_metrics_lookup 
ON kafka_ops.processing_metrics (instance_id, metric_name, recorded_at);

CREATE INDEX IF NOT EXISTS idx_processing_metrics_labels 
ON kafka_ops.processing_metrics USING GIN (labels);

-- Table for storing error logs
CREATE TABLE IF NOT EXISTS kafka_ops.error_logs (
    id SERIAL PRIMARY KEY,
    instance_id VARCHAR(255) NOT NULL,
    error_type VARCHAR(255) NOT NULL,
    error_message TEXT NOT NULL,
    context JSONB,
    occurred_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    resolved_at TIMESTAMP WITH TIME ZONE
);

-- Index for error log queries
CREATE INDEX IF NOT EXISTS idx_error_logs_lookup 
ON kafka_ops.error_logs (instance_id, error_type, occurred_at);

CREATE INDEX IF NOT EXISTS idx_error_logs_unresolved 
ON kafka_ops.error_logs (occurred_at) WHERE resolved_at IS NULL;

-- Table for storing consumer lag metrics
CREATE TABLE IF NOT EXISTS kafka_ops.consumer_lag_history (
    id SERIAL PRIMARY KEY,
    topic VARCHAR(255) NOT NULL,
    partition_id INTEGER NOT NULL,
    consumer_group VARCHAR(255) NOT NULL,
    lag_value BIGINT NOT NULL,
    high_water_mark BIGINT NOT NULL,
    current_offset BIGINT NOT NULL,
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index for lag history queries
CREATE INDEX IF NOT EXISTS idx_consumer_lag_history_lookup 
ON kafka_ops.consumer_lag_history (topic, partition_id, consumer_group, recorded_at);

-- Function to update offset with upsert
CREATE OR REPLACE FUNCTION kafka_ops.upsert_consumer_offset(
    p_topic VARCHAR(255),
    p_partition INTEGER,
    p_consumer_group VARCHAR(255),
    p_offset BIGINT
) RETURNS void AS $$
BEGIN
    INSERT INTO kafka_ops.consumer_offsets (topic, partition_id, consumer_group, offset_value)
    VALUES (p_topic, p_partition, p_consumer_group, p_offset)
    ON CONFLICT (topic, partition_id, consumer_group)
    DO UPDATE SET 
        offset_value = EXCLUDED.offset_value,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Function to record processing metrics
CREATE OR REPLACE FUNCTION kafka_ops.record_metric(
    p_instance_id VARCHAR(255),
    p_metric_name VARCHAR(255),
    p_metric_value DOUBLE PRECISION,
    p_labels JSONB DEFAULT '{}'::jsonb
) RETURNS void AS $$
BEGIN
    INSERT INTO kafka_ops.processing_metrics (instance_id, metric_name, metric_value, labels)
    VALUES (p_instance_id, p_metric_name, p_metric_value, p_labels);
END;
$$ LANGUAGE plpgsql;

-- Function to log errors
CREATE OR REPLACE FUNCTION kafka_ops.log_error(
    p_instance_id VARCHAR(255),
    p_error_type VARCHAR(255),
    p_error_message TEXT,
    p_context JSONB DEFAULT '{}'::jsonb
) RETURNS void AS $$
BEGIN
    INSERT INTO kafka_ops.error_logs (instance_id, error_type, error_message, context)
    VALUES (p_instance_id, p_error_type, p_error_message, p_context);
END;
$$ LANGUAGE plpgsql;

-- Function to record consumer lag
CREATE OR REPLACE FUNCTION kafka_ops.record_consumer_lag(
    p_topic VARCHAR(255),
    p_partition INTEGER,
    p_consumer_group VARCHAR(255),
    p_lag BIGINT,
    p_high_water_mark BIGINT,
    p_current_offset BIGINT
) RETURNS void AS $$
BEGIN
    INSERT INTO kafka_ops.consumer_lag_history 
    (topic, partition_id, consumer_group, lag_value, high_water_mark, current_offset)
    VALUES (p_topic, p_partition, p_consumer_group, p_lag, p_high_water_mark, p_current_offset);
END;
$$ LANGUAGE plpgsql;

-- Create user for the application
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'kafka_user') THEN
        CREATE ROLE kafka_user WITH LOGIN PASSWORD 'secure_kafka_password';
    END IF;
END
$$;

-- Grant permissions
GRANT USAGE ON SCHEMA kafka_ops TO kafka_user;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA kafka_ops TO kafka_user;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA kafka_ops TO kafka_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA kafka_ops TO kafka_user;

-- Set default privileges for future objects
ALTER DEFAULT PRIVILEGES IN SCHEMA kafka_ops GRANT ALL ON TABLES TO kafka_user;
ALTER DEFAULT PRIVILEGES IN SCHEMA kafka_ops GRANT ALL ON SEQUENCES TO kafka_user;
ALTER DEFAULT PRIVILEGES IN SCHEMA kafka_ops GRANT EXECUTE ON FUNCTIONS TO kafka_user;

-- Create views for monitoring
CREATE OR REPLACE VIEW kafka_ops.current_consumer_offsets AS
SELECT 
    topic,
    partition_id,
    consumer_group,
    offset_value,
    updated_at,
    EXTRACT(EPOCH FROM (NOW() - updated_at)) AS seconds_since_update
FROM kafka_ops.consumer_offsets
ORDER BY topic, partition_id;

CREATE OR REPLACE VIEW kafka_ops.recent_errors AS
SELECT 
    instance_id,
    error_type,
    error_message,
    context,
    occurred_at,
    resolved_at,
    CASE WHEN resolved_at IS NULL THEN 'ACTIVE' ELSE 'RESOLVED' END AS status
FROM kafka_ops.error_logs
WHERE occurred_at > NOW() - INTERVAL '24 hours'
ORDER BY occurred_at DESC;

CREATE OR REPLACE VIEW kafka_ops.lag_summary AS
SELECT 
    topic,
    consumer_group,
    SUM(lag_value) AS total_lag,
    AVG(lag_value) AS avg_lag,
    MAX(lag_value) AS max_lag,
    COUNT(*) AS partition_count,
    MAX(recorded_at) AS last_recorded
FROM kafka_ops.consumer_lag_history
WHERE recorded_at > NOW() - INTERVAL '1 hour'
GROUP BY topic, consumer_group
ORDER BY total_lag DESC;

-- Insert initial monitoring data
INSERT INTO kafka_ops.processing_metrics (instance_id, metric_name, metric_value, labels)
VALUES ('system', 'database_initialized', 1, '{"version": "1.0", "timestamp": "' || NOW() || '"}');

COMMIT;

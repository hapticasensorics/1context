CREATE MATERIALIZED VIEW IF NOT EXISTS perception.object_density_1m
WITH (timescaledb.continuous) AS
SELECT
  user_id,
  lane_id,
  kind,
  role,
  privacy_class,
  time_bucket(INTERVAL '1 minute', event_start) AS bucket_start,
  count(*) AS object_count,
  min(event_start) AS first_event_start,
  max(event_end) AS last_event_end,
  avg(importance_score) AS avg_importance
FROM perception.objects
WHERE valid_to IS NULL
GROUP BY
  user_id,
  lane_id,
  kind,
  role,
  privacy_class,
  time_bucket(INTERVAL '1 minute', event_start)
WITH NO DATA;

CREATE INDEX IF NOT EXISTS object_density_1m_lookup_idx
ON perception.object_density_1m
  (user_id, lane_id, bucket_start DESC, kind, role, privacy_class);

DO $$
BEGIN
  BEGIN
    PERFORM add_continuous_aggregate_policy(
      'perception.object_density_1m',
      start_offset => INTERVAL '7 days',
      end_offset => INTERVAL '1 minute',
      schedule_interval => INTERVAL '1 minute',
      if_not_exists => TRUE
    );
  EXCEPTION
    WHEN undefined_function THEN
      BEGIN
        PERFORM add_continuous_aggregate_policy(
          'perception.object_density_1m',
          start_offset => INTERVAL '7 days',
          end_offset => INTERVAL '1 minute',
          schedule_interval => INTERVAL '1 minute'
        );
      EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
          NULL;
      END;
    WHEN duplicate_object OR unique_violation THEN
      NULL;
  END;
END $$;

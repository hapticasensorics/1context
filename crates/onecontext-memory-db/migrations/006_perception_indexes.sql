CREATE INDEX IF NOT EXISTS objects_user_live_time_idx
ON perception.objects (user_id, event_start, lane_id, object_id)
INCLUDE (
  event_end,
  source_id,
  source_record_id,
  series_id,
  kind,
  role,
  privacy_class,
  body_type,
  blob_id,
  importance_score
)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_lane_live_time_idx
ON perception.objects (user_id, lane_id, event_start, object_id)
INCLUDE (
  event_end,
  source_id,
  source_record_id,
  series_id,
  kind,
  role,
  privacy_class,
  display_title,
  display_text,
  body_type,
  blob_id,
  importance_score
)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_lane_kind_live_time_idx
ON perception.objects (user_id, lane_id, kind, event_start DESC)
INCLUDE (object_id, series_id, role, privacy_class, event_end, display_title, body_type, blob_id)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_series_live_time_idx
ON perception.objects (user_id, series_id, event_start DESC)
INCLUDE (object_id, kind, role, privacy_class, event_end, display_title, body_type)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_lane_series_live_time_idx
ON perception.objects (user_id, lane_id, series_id, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_source_live_time_idx
ON perception.objects (user_id, source_id, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_source_record_key_live_idx
ON perception.objects (source_id, source_record_key, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_kind_live_time_idx
ON perception.objects (user_id, kind, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_role_live_time_idx
ON perception.objects (user_id, role, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_privacy_live_time_idx
ON perception.objects (user_id, privacy_class, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_event_range_live_gist_idx
ON perception.objects
USING GIST (user_id, event_range)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_gin_idx
ON perception.objects
USING GIN (payload jsonb_path_ops);

CREATE INDEX IF NOT EXISTS objects_payload_app_idx
ON perception.objects ((payload->>'app'))
WHERE payload ? 'app' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_thread_idx
ON perception.objects ((payload->>'thread_id'))
WHERE payload ? 'thread_id' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_session_idx
ON perception.objects ((payload->>'session_id'))
WHERE payload ? 'session_id' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_browser_app_idx
ON perception.objects ((payload->'browser'->>'app'))
WHERE payload ? 'browser' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_file_language_idx
ON perception.objects ((payload->'file'->>'language'))
WHERE payload ? 'file' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_search_vector_live_idx
ON perception.objects
USING GIN (search_vector)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_display_text_trgm_idx
ON perception.objects
USING GIN (display_text gin_trgm_ops)
WHERE display_text IS NOT NULL AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_text_value_trgm_idx
ON perception.objects
USING GIN (text_value gin_trgm_ops)
WHERE text_value IS NOT NULL AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS source_records_user_object_hydrate_idx
ON perception.source_records (user_id, object_id)
INCLUDE (
  object_event_start,
  series_id,
  source_record_id,
  source_record_key,
  source_record_hash,
  first_seen_at,
  last_seen_at,
  seen_count
);

use anyhow::Result;

use crate::{
    fixture::AttentionFixture,
    model::{CandidateFrameSet, CandidateState},
};

pub fn build_candidates(fixture: &AttentionFixture) -> Result<Vec<CandidateState>> {
    let Some(frame_set) = preferred_frame_set(fixture) else {
        return Ok(build_event_candidates(fixture));
    };
    let mut candidates = Vec::with_capacity(frame_set.count);

    for frame_index in 1..=frame_set.count {
        let t_ms = frame_time_ms(frame_set, frame_index);
        let frame_id = format!("{}:frame-{frame_index:03}", frame_set.id);
        let image_ref = image_ref_for_frame(frame_set, frame_index);
        let image_ref = format!("{}/{}", frame_set.root, image_ref);
        let nearby_events = fixture
            .events
            .iter()
            .filter(|event| is_nearby_event(t_ms, event))
            .cloned()
            .collect();

        candidates.push(CandidateState {
            id: format!("candidate-{frame_id}"),
            frame_id,
            t_ms,
            image_ref,
            nearby_events,
            signals: Vec::new(),
            attention_score: 0.0,
            memory_value_score: 0.0,
        });
    }

    Ok(candidates)
}

fn build_event_candidates(fixture: &AttentionFixture) -> Vec<CandidateState> {
    let mut times = fixture
        .events
        .iter()
        .map(|event| event.t_ms.min(fixture.session.fixture.duration_ms))
        .collect::<Vec<_>>();
    times.sort_unstable();
    times.dedup_by(|left, right| left.abs_diff(*right) <= 250);

    if times.is_empty() {
        times.push(0);
    }

    times
        .into_iter()
        .enumerate()
        .map(|(index, t_ms)| {
            let nearby_events = fixture
                .events
                .iter()
                .filter(|event| is_nearby_event(t_ms, event))
                .cloned()
                .collect();
            let frame_id = format!("event-candidate-{index:03}");

            CandidateState {
                id: format!("candidate-{frame_id}"),
                frame_id,
                t_ms,
                image_ref: "media/unavailable".to_string(),
                nearby_events,
                signals: Vec::new(),
                attention_score: 0.0,
                memory_value_score: 0.0,
            }
        })
        .collect()
}

fn frame_time_ms(frame_set: &CandidateFrameSet, frame_index: usize) -> u64 {
    let fps = frame_set.fps.max(0.1) as f64;
    (((frame_index.saturating_sub(1)) as f64 / fps) * 1000.0).round() as u64
}

fn image_ref_for_frame(frame_set: &CandidateFrameSet, frame_index: usize) -> String {
    frame_set
        .naming
        .replace("{index:06}", &format!("{frame_index:06}"))
        .replace("{index:03}", &format!("{frame_index:03}"))
        .replace("{index}", &frame_index.to_string())
}

fn is_nearby_event(candidate_t_ms: u64, event: &crate::model::CaptureEvent) -> bool {
    const PRIOR_WINDOW_MS: u64 = 1_500;
    const NEXT_WINDOW_MS: u64 = 1_500;
    const ACTION_OUTCOME_WINDOW_MS: u64 = 2_500;

    let post_window_ms = if is_action_event(&event.event_type) {
        ACTION_OUTCOME_WINDOW_MS
    } else {
        NEXT_WINDOW_MS
    };
    let event_end_ms = event
        .duration_ms
        .map(|duration_ms| event.t_ms.saturating_add(duration_ms))
        .unwrap_or(event.t_ms);

    candidate_t_ms.saturating_add(PRIOR_WINDOW_MS) >= event.t_ms
        && candidate_t_ms <= event_end_ms.saturating_add(post_window_ms)
}

fn is_action_event(event_type: &str) -> bool {
    event_type.contains("keyboard_activity")
        || event_type.contains("pointer")
        || event_type.contains("shortcut")
}

fn preferred_frame_set(fixture: &AttentionFixture) -> Option<&CandidateFrameSet> {
    fixture
        .session
        .media
        .candidate_frame_sets
        .iter()
        .find(|set| set.id == "2fps")
        .or_else(|| fixture.session.media.candidate_frame_sets.first())
}

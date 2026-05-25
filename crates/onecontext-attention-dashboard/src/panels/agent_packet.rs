use eframe::egui;

use crate::schema::{
    AgentAttentionPacket, AlgorithmRunSummary, AskableEvidence, AttentionDebtItem,
    AttentionFilterOutput, SourceConflict,
};

use super::saved_states::{format_time_ms, key_value, render_json_map, score_pill};

pub(super) fn render_agent_packet(ui: &mut egui::Ui, packet: &AgentAttentionPacket) {
    key_value(
        ui,
        "time range",
        &format!(
            "{} to {}",
            format_time_ms(packet.time_range_ms[0]),
            format_time_ms(packet.time_range_ms[1])
        ),
    );
    score_pill(ui, "confidence", packet.confidence);
    if !packet.activity_summary.is_empty() {
        ui.separator();
        ui.label(&packet.activity_summary);
    }

    if packet.important_observations.is_empty()
        && packet.extracted_text.is_empty()
        && packet.composites.is_empty()
        && packet.askable_evidence.is_empty()
        && packet.extra.is_empty()
    {
        ui.label("No agent-facing observations yet.");
        return;
    }

    if !packet.important_observations.is_empty() {
        ui.collapsing(
            format!(
                "Important Observations ({})",
                packet.important_observations.len()
            ),
            |ui| {
                for observation in &packet.important_observations {
                    ui.group(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(if observation.kind.is_empty() {
                                "observation"
                            } else {
                                &observation.kind
                            });
                            score_pill(ui, "confidence", observation.confidence);
                            if !observation.proof_tier.is_empty() {
                                ui.label(&observation.proof_tier);
                            }
                        });
                        if !observation.summary.is_empty() {
                            ui.label(&observation.summary);
                        }
                        key_value(ui, "evidence state", &observation.evidence_state_id);
                        if !observation.extra.is_empty() {
                            render_json_map(ui, &observation.extra);
                        }
                    });
                }
            },
        );
    }

    if !packet.extracted_text.is_empty() {
        ui.collapsing(
            format!("Extracted Text ({})", packet.extracted_text.len()),
            |ui| {
                for text in &packet.extracted_text {
                    ui.group(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(if text.source.is_empty() {
                                "text"
                            } else {
                                &text.source
                            });
                            score_pill(ui, "confidence", text.confidence);
                            if text.sensitive.unwrap_or(false) {
                                ui.colored_label(egui::Color32::from_rgb(210, 64, 64), "sensitive");
                            }
                        });
                        if !text.text.is_empty() {
                            ui.label(&text.text);
                        }
                        if !text.state_ids.is_empty() {
                            key_value(ui, "states", &text.state_ids.join(", "));
                        }
                        if !text.extra.is_empty() {
                            render_json_map(ui, &text.extra);
                        }
                    });
                }
            },
        );
    }

    if !packet.composites.is_empty() {
        ui.collapsing(format!("Composites ({})", packet.composites.len()), |ui| {
            for composite in &packet.composites {
                ui.group(|ui| {
                    key_value(ui, "id", &composite.id);
                    key_value(ui, "type", &composite.composite_type);
                    if !composite.summary.is_empty() {
                        ui.label(&composite.summary);
                    }
                    if !composite.extra.is_empty() {
                        render_json_map(ui, &composite.extra);
                    }
                });
            }
        });
    }

    if !packet.askable_evidence.is_empty() {
        ui.collapsing(
            format!("Askable Evidence ({})", packet.askable_evidence.len()),
            |ui| render_askable_evidence(ui, &packet.askable_evidence),
        );
    }

    if !packet.extra.is_empty() {
        ui.collapsing("Extra Packet Fields", |ui| {
            render_json_map(ui, &packet.extra);
        });
    }
}

pub(super) fn render_source_conflicts(ui: &mut egui::Ui, conflicts: &[SourceConflict]) {
    if conflicts.is_empty() {
        ui.label("No source conflicts reported.");
        return;
    }

    for conflict in conflicts {
        ui.group(|ui| {
            let title = if !conflict.id.is_empty() {
                conflict.id.as_str()
            } else {
                "source conflict"
            };
            ui.strong(title);
            if let Some(t_ms) = conflict.t_ms {
                key_value(ui, "time", &format_time_ms(t_ms));
            }
            if let Some(candidate_id) = &conflict.candidate_id {
                key_value(ui, "candidate", candidate_id);
            }
            if let Some(saved_state_id) = &conflict.saved_state_id {
                key_value(ui, "saved state", saved_state_id);
            }
            if let Some(severity) = &conflict.severity {
                key_value(ui, "severity", severity);
            }
            if let Some(source_a) = &conflict.source_a {
                key_value(ui, "source a", source_a);
            }
            if let Some(source_b) = &conflict.source_b {
                key_value(ui, "source b", source_b);
            }
            if let Some(text) = &conflict.conflict {
                key_value(ui, "conflict", text);
            }
            if let Some(resolution) = &conflict.resolution {
                key_value(ui, "resolution", resolution);
            }
            if let Some(explanation) = &conflict.explanation {
                ui.label(explanation);
            }
            if !conflict.extra.is_empty() {
                render_json_map(ui, &conflict.extra);
            }
        });
    }
}

pub(super) fn render_attention_debt(ui: &mut egui::Ui, debts: &[AttentionDebtItem]) {
    if debts.is_empty() {
        ui.label("No outstanding attention debt.");
        return;
    }

    for debt in debts {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(if debt.kind.is_empty() {
                    "attention debt"
                } else {
                    &debt.kind
                });
                if !debt.status.is_empty() {
                    ui.label(&debt.status);
                }
                if let Some(t_ms) = debt.t_ms {
                    ui.label(format_time_ms(t_ms));
                }
            });
            key_value(ui, "id", &debt.id);
            if let Some(candidate_id) = &debt.candidate_id {
                key_value(ui, "candidate", candidate_id);
            }
            if let Some(saved_state_id) = &debt.saved_state_id {
                key_value(ui, "saved state", saved_state_id);
            }
            if let Some(description) = &debt.description {
                ui.label(description);
            }
            if let Some(resolution) = &debt.resolution {
                key_value(ui, "resolution", resolution);
            }
            if let Some(explanation) = &debt.explanation {
                ui.small(explanation);
            }
            if !debt.extra.is_empty() {
                render_json_map(ui, &debt.extra);
            }
        });
    }
}

pub(super) fn render_algorithms(ui: &mut egui::Ui, output: &AttentionFilterOutput) {
    if output.algorithms.is_empty() {
        ui.label("No algorithm run summaries yet.");
        return;
    }

    for algorithm in &output.algorithms {
        ui.group(|ui| {
            render_algorithm_summary(ui, algorithm);
        });
    }
}

fn render_algorithm_summary(ui: &mut egui::Ui, algorithm: &AlgorithmRunSummary) {
    let title = if !algorithm.name.is_empty() {
        algorithm.name.as_str()
    } else if !algorithm.id.is_empty() {
        algorithm.id.as_str()
    } else {
        "algorithm"
    };
    ui.horizontal_wrapped(|ui| {
        ui.strong(title);
        if let Some(version) = &algorithm.version {
            ui.label(version);
        }
        if let Some(enabled) = algorithm.enabled {
            ui.label(if enabled { "enabled" } else { "disabled" });
        }
        if let Some(status) = &algorithm.status {
            ui.label(status);
        }
    });
    if let Some(summary) = &algorithm.summary {
        ui.label(summary);
    }
    if let Some(explanation) = &algorithm.explanation {
        ui.small(explanation);
    }

    ui.horizontal_wrapped(|ui| {
        if let Some(count) = algorithm.candidates_considered {
            ui.label(format!("candidates: {count}"));
        }
        if let Some(count) = algorithm.saved_count {
            ui.label(format!("saved: {count}"));
        }
        if let Some(count) = algorithm.merged_count {
            ui.label(format!("merged: {count}"));
        }
        if let Some(count) = algorithm.dropped_count {
            ui.label(format!("dropped: {count}"));
        }
        if let Some(runtime_ms) = algorithm.runtime_ms {
            ui.label(format!("runtime: {:.1}ms", runtime_ms));
        }
    });

    if !algorithm.score_components.is_empty() {
        ui.collapsing("Score Components", |ui| {
            render_json_map(ui, &algorithm.score_components);
        });
    }
    if !algorithm.votes.is_empty() {
        ui.collapsing(format!("Votes ({})", algorithm.votes.len()), |ui| {
            for vote in &algorithm.votes {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(if vote.algorithm.is_empty() {
                        title
                    } else {
                        &vote.algorithm
                    });
                    ui.label(if vote.vote.is_empty() {
                        "vote"
                    } else {
                        &vote.vote
                    });
                    score_pill(ui, "strength", vote.strength);
                });
                if !vote.reason.is_empty() {
                    ui.small(&vote.reason);
                }
            }
        });
    }
    if !algorithm.extra.is_empty() {
        ui.collapsing("Extra Fields", |ui| {
            render_json_map(ui, &algorithm.extra);
        });
    }
}

fn render_askable_evidence(ui: &mut egui::Ui, evidence: &[AskableEvidence]) {
    for item in evidence {
        ui.group(|ui| {
            key_value(
                ui,
                "label",
                if item.label.is_empty() {
                    "evidence"
                } else {
                    &item.label
                },
            );
            key_value(ui, "ref", &item.path);
            key_value(ui, "tier", &item.proof_tier);
            if !item.extra.is_empty() {
                render_json_map(ui, &item.extra);
            }
        });
    }
}

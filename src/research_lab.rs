//! Durable registry, datasets and experiment archive behind one typed action API.
use crate::{asset_registry::RegistryRevision, research_store::ResearchStore, types::now_ms};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LabAction {
    AnnouncementPut(crate::research_events::Announcement),
    RegistryPut(RegistryRevision),
    RegistryPair {
        revision_id: String,
        left: String,
        right: String,
        as_of_ms: u64,
    },
    DatasetAppend {
        dataset_id: String,
        chunk_id: String,
        frames: Vec<crate::research_engine::ScanRequest>,
    },
    DatasetReplay {
        dataset_id: String,
        run_id: String,
        after_sequence: u64,
        limit_chunks: usize,
    },
    Run {
        id: String,
        model: String,
        input: Value,
    },
    List {
        namespace: String,
        after_sequence: u64,
        limit: usize,
    },
    Get {
        namespace: String,
        id: String,
    },
    Integrity,
}
fn dataset_namespace(id: &str) -> Result<String> {
    ensure!(
        crate::research_store::valid_id(id) && id.len() <= 80,
        "invalid dataset ID"
    );
    Ok(format!("dataset.{id}"))
}
fn readable(namespace: &str) -> bool {
    matches!(
        namespace,
        "registry" | "runs" | "events" | "control" | "captures" | "announcements"
    ) || namespace.starts_with("dataset.")
}

pub fn execute(store: &ResearchStore, action: LabAction) -> Result<Value> {
    let _operation = store.operation_lock()?;
    match action {
        LabAction::AnnouncementPut(event) => {
            event.validate(now_ms())?;
            let payload = serde_json::to_value(&event)?;
            if let Some(existing) = store.get("announcements", &event.id)? {
                ensure!(
                    existing.payload == payload,
                    "event ID exists with different content; corrections require new IDs"
                );
                return Ok(serde_json::to_value(existing)?);
            }
            Ok(serde_json::to_value(store.insert(
                "announcements",
                &event.id,
                now_ms(),
                &payload,
            )?)?)
        }
        LabAction::RegistryPut(revision) => {
            revision.validate().map_err(anyhow::Error::msg)?;
            ensure!(
                revision.known_at_ms <= now_ms(),
                "registry knowledge time is in the future"
            );
            Ok(serde_json::to_value(store.insert(
                "registry",
                &revision.id,
                now_ms(),
                &serde_json::to_value(&revision)?,
            )?)?)
        }
        LabAction::RegistryPair {
            revision_id,
            left,
            right,
            as_of_ms,
        } => {
            let doc = store
                .get("registry", &revision_id)?
                .ok_or_else(|| anyhow::anyhow!("registry not found"))?;
            let revision: RegistryRevision = serde_json::from_value(doc.payload)?;
            let (a, b, r) = revision
                .pair(&left, &right, as_of_ms)
                .map_err(anyhow::Error::msg)?;
            Ok(
                json!({"buy":a,"sell":b,"relationship":r,"revision_id":revision_id,"recorded_at_ms":doc.recorded_at_ms,"knowledge_basis":"caller_attestation_not_independent_verification"}),
            )
        }
        LabAction::DatasetAppend {
            dataset_id,
            chunk_id,
            frames,
        } => {
            let namespace = dataset_namespace(&dataset_id)?;
            crate::research_engine::replay(&crate::research_engine::ReplayRequest {
                frames: frames.clone(),
            })
            .map_err(anyhow::Error::msg)?;
            let (last, count) = store.tail_and_count(&namespace)?;
            ensure!(
                count < 100,
                "dataset limit is 100 chunks; create a new dataset"
            );
            if let Some(last) = last {
                let end = last.payload["last_as_of_ms"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("invalid dataset metadata"))?;
                ensure!(
                    frames[0].as_of_ms >= end,
                    "dataset chunks must be chronological"
                );
            }
            let payload = json!({"dataset_id":dataset_id,"chunk_id":chunk_id,"first_as_of_ms":frames[0].as_of_ms,"last_as_of_ms":frames.last().unwrap().as_of_ms,"frames":frames,"schema_version":"scan-dataset/v1","provenance":"caller_supplied_evidence"});
            Ok(serde_json::to_value(store.insert(
                &namespace,
                &chunk_id,
                now_ms(),
                &payload,
            )?)?)
        }
        LabAction::DatasetReplay {
            dataset_id,
            run_id,
            after_sequence,
            limit_chunks,
        } => {
            ensure!(
                (1..=16).contains(&limit_chunks),
                "limit_chunks must be 1..16"
            );
            let docs = store.list(
                &dataset_namespace(&dataset_id)?,
                after_sequence,
                limit_chunks,
            )?;
            ensure!(!docs.is_empty(), "no dataset chunks after cursor");
            let mut chunks = Vec::new();
            for doc in &docs {
                let frames = serde_json::from_value(doc.payload["frames"].clone())?;
                let result =
                    crate::research_engine::replay(&crate::research_engine::ReplayRequest {
                        frames,
                    })
                    .map_err(anyhow::Error::msg)?;
                chunks.push(json!({"chunk_id":doc.id,"sequence":doc.sequence,"result":result}));
            }
            let payload = json!({"model":"dataset-replay/v1","dataset_id":dataset_id,"after_sequence":after_sequence,"next_sequence":docs.last().unwrap().sequence,"chunks":chunks,"orders_supported":false});
            Ok(serde_json::to_value(store.insert(
                "runs",
                &run_id,
                now_ms(),
                &payload,
            )?)?)
        }
        LabAction::Run { id, model, input } => {
            let started = now_ms();
            let result = evaluate(&model, input.clone());
            let output = match result {
                Ok(output) => json!({"status":"completed","result":output}),
                Err(error) => json!({"status":"failed","error":error.to_string()}),
            };
            let payload = json!({"model":model,"input":input,"output":output,"started_at_ms":started,"finished_at_ms":now_ms(),"package_version":env!("CARGO_PKG_VERSION"),"build_revision":crate::BUILD_REVISION,"orders_supported":false});
            Ok(serde_json::to_value(store.insert(
                "runs",
                &id,
                now_ms(),
                &payload,
            )?)?)
        }
        LabAction::List {
            namespace,
            after_sequence,
            limit,
        } => {
            ensure!(readable(&namespace), "namespace not readable");
            let docs = store.list(&namespace, after_sequence, limit)?;
            Ok(
                json!({"documents":docs,"next_sequence":docs.last().map(|d|d.sequence).unwrap_or(after_sequence)}),
            )
        }
        LabAction::Get { namespace, id } => {
            ensure!(readable(&namespace), "namespace not readable");
            Ok(json!({"document":store.get(&namespace,&id)?}))
        }
        LabAction::Integrity => store.integrity(),
    }
}

pub fn evaluate(model: &str, input: Value) -> Result<Value> {
    Ok(match model {
        "allocated-spot-portfolio/v1" => {
            crate::research_portfolio::simulate(&serde_json::from_value(input)?)?
        }
        "announcement-window/v1" => crate::research_events::study(&serde_json::from_value(input)?)?,
        "spot-derivative-basis/v1" | "unit-premium/v1" => {
            let request: crate::relative_value::RelativeRequest = serde_json::from_value(input)?;
            ensure!(request.model == model, "model discriminator mismatch");
            crate::relative_value::relative(&request).map_err(anyhow::Error::msg)?
        }
        "funding-rate-comparison/v1" => {
            crate::relative_value::funding(&serde_json::from_value(input)?)
                .map_err(anyhow::Error::msg)?
        }
        "same-asset-spot/v1" => serde_json::to_value(
            crate::research_engine::scan(&serde_json::from_value(input)?)
                .map_err(anyhow::Error::msg)?,
        )?,
        "candidate-screen/v1" => serde_json::to_value(
            crate::opportunity_scan::evaluate(&serde_json::from_value(input)?)
                .map_err(anyhow::Error::msg)?,
        )?,
        "scenario-replay/v1" => serde_json::to_value(
            crate::research_engine::replay(&serde_json::from_value(input)?)
                .map_err(anyhow::Error::msg)?,
        )?,
        "prefunded-taker-scenario/v1" => serde_json::to_value(
            crate::paper::simulate(&serde_json::from_value(input)?).map_err(anyhow::Error::msg)?,
        )?,
        _ => anyhow::bail!("unknown model version"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dataset_replay_and_failed_experiments_are_durable() {
        let db = ResearchStore::memory();
        let frame = crate::research_engine::tests::fixture();
        execute(
            &db,
            LabAction::DatasetAppend {
                dataset_id: "test".into(),
                chunk_id: "one".into(),
                frames: vec![frame],
            },
        )
        .unwrap();
        let run = execute(
            &db,
            LabAction::DatasetReplay {
                dataset_id: "test".into(),
                run_id: "run".into(),
                after_sequence: 0,
                limit_chunks: 1,
            },
        )
        .unwrap();
        assert_eq!(run["payload"]["chunks"].as_array().unwrap().len(), 1);
        let failed = execute(
            &db,
            LabAction::Run {
                id: "bad".into(),
                model: "unknown".into(),
                input: json!({}),
            },
        )
        .unwrap();
        assert_eq!(failed["payload"]["output"]["status"], "failed");
    }
}

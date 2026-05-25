mod codex_notify;

use anyhow::{anyhow, Context, Result};
use chrono::{Duration, SecondsFormat, Utc};
use codex_notify::{
    decide_codex_notification_dispatch, CodexDispatchRequest, CodexRuntimeStatus,
    CodexSupervisorPolicy,
};
use onecontext_wiki_core::{
    agent_mail::{
        AgentGrantPolicy, AgentIdentifyRequest, AgentMailStore, AgentRecord, CodexSteeringPayload,
        DeliveryAttemptStatus, DeliveryState, MailAddress, MailInjectionResult, MessageAcceptance,
        MessageAttachmentRef, MessageBodyRef, MessageEnvelope, MessagePageRef,
        NotificationAttemptStatus, SendMailOptions,
    },
    PageCreateOptions, TalkAppendRequest, TalkAttachmentInput, TalkDeliveryMode, WikiCore,
    WikiInventory,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::io::Write;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = run(args.clone()) {
        let message = format!("{error:#}");
        let command = command_name(&args).unwrap_or("unknown");
        let mut error = json!({
            "code": error_code(&message),
            "message": message,
        });
        if let Some(details) = error_details(error["message"].as_str().unwrap_or_default()) {
            error["details"] = details;
        }
        let repair_hints = repair_hints(error["message"].as_str().unwrap_or_default());
        let envelope = json!({
            "schema_version": 1,
            "status": "error",
            "operation": operation_for_command(command),
            "command": command,
            "error": error,
            "repair_hints": repair_hints,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| {
                "{\"schema_version\":1,\"status\":\"error\",\"operation\":\"wiki.unknown\"}"
                    .to_string()
            })
        );
        std::process::exit(1);
    }
}

fn run(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let root = take_flag_value(&mut args, "onecontext-wiki", "--root")?
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("runtime-test/1Context"));
    let command = args
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("missing command"))?;
    args.remove(0);
    let command = canonical_command(&command).to_string();
    let core = WikiCore::new(root.clone());

    match command.as_str() {
        "ensure" => {
            reject_extra_args("ensure", &args)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.ensure",
                "created": {
                    "runtime_dirs": core.ensure_runtime_dirs().map(|_| true)?
                }
            }))
        }
        "status" => {
            reject_extra_args("status", &args)?;
            print_json(&core.status()?)
        }
        "validate" => {
            reject_extra_args("validate", &args)?;
            print_json(&core.validate()?)
        }
        "list" => {
            reject_extra_args("list", &args)?;
            print_json(&core.inventory()?)
        }
        "page-status" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("page-status requires <page>"))?;
            args.remove(0);
            reject_extra_args("page-status", &args)?;
            print_json(&core.page_status(&page)?)
        }
        "page-open" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("page-open requires <page>"))?;
            args.remove(0);
            reject_extra_args("page-open", &args)?;
            print_json(&core.open_page(&page)?)
        }
        "page-create" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("page-create requires <page>"))?;
            args.remove(0);
            let options = PageCreateOptions {
                title: take_flag_value(&mut args, "page-create", "--title")?,
                slug: take_flag_value(&mut args, "page-create", "--slug")?,
                route: take_flag_value(&mut args, "page-create", "--route")?,
                family_group: take_flag_value(&mut args, "page-create", "--family-group")?,
                family_group_title: take_flag_value(
                    &mut args,
                    "page-create",
                    "--family-group-title",
                )?,
                family_id: take_flag_value(&mut args, "page-create", "--family-id")?,
                family_title: take_flag_value(&mut args, "page-create", "--family-title")?,
                page_type: take_flag_value(&mut args, "page-create", "--type")?,
                template: take_flag_value(&mut args, "page-create", "--template")?,
                talk_conventions_template: take_flag_value(
                    &mut args,
                    "page-create",
                    "--talk-conventions-template",
                )?,
                talk_curator_template: take_flag_value(
                    &mut args,
                    "page-create",
                    "--talk-curator-template",
                )?,
                summary: take_flag_value(&mut args, "page-create", "--summary")?,
                nav_order: take_i64_flag(&mut args, "page-create", "--nav-order")?,
                nav_section: take_flag_value(&mut args, "page-create", "--nav-section")?,
            };
            reject_extra_args("page-create", &args)?;
            print_json(&core.create_page_with_options(&page, options, None)?)
        }
        "page-create-all" => {
            reject_extra_args("page-create-all", &args)?;
            print_json(&core.create_all_pages()?)
        }
        "page-write-body" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("page-write-body requires <page>"))?;
            args.remove(0);
            let body = read_text_arg(&mut args, "page-write-body", "--body", "--body-file")?
                .ok_or_else(|| anyhow!("page-write-body requires --body or --body-file"))?;
            let expected =
                take_flag_value(&mut args, "page-write-body", "--expected-source-sha256")?;
            reject_extra_args("page-write-body", &args)?;
            print_json(&core.write_page_body(&page, &body, expected.as_deref())?)
        }
        "page-patch-body" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("page-patch-body requires <page>"))?;
            args.remove(0);
            let find = read_text_arg(&mut args, "page-patch-body", "--find", "--find-file")?
                .ok_or_else(|| anyhow!("page-patch-body requires --find or --find-file"))?;
            let replace =
                read_text_arg(&mut args, "page-patch-body", "--replace", "--replace-file")?
                    .ok_or_else(|| {
                        anyhow!("page-patch-body requires --replace or --replace-file")
                    })?;
            let expected =
                take_flag_value(&mut args, "page-patch-body", "--expected-source-sha256")?;
            reject_extra_args("page-patch-body", &args)?;
            print_json(&core.patch_page_body(&page, &find, &replace, expected.as_deref())?)
        }
        "asset-add" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("asset-add requires <page>"))?;
            args.remove(0);
            let file = take_flag_value(&mut args, "asset-add", "--file")?
                .map(PathBuf::from)
                .ok_or_else(|| anyhow!("asset-add requires --file"))?;
            let filename = take_flag_value(&mut args, "asset-add", "--filename")?;
            let purpose = take_flag_value(&mut args, "asset-add", "--purpose")?;
            let caption = take_flag_value(&mut args, "asset-add", "--caption")?;
            let alt_text = take_flag_value(&mut args, "asset-add", "--alt-text")?;
            reject_extra_args("asset-add", &args)?;
            print_json(&core.add_page_asset(
                &page,
                &file,
                filename.as_deref(),
                purpose.as_deref(),
                caption.as_deref(),
                alt_text.as_deref(),
            )?)
        }
        "asset-list" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("asset-list requires <page>"))?;
            args.remove(0);
            reject_extra_args("asset-list", &args)?;
            print_json(&core.list_page_assets(&page)?)
        }
        "reference-list" => {
            let page = if args.is_empty() {
                None
            } else {
                Some(args.remove(0))
            };
            reject_extra_args("reference-list", &args)?;
            print_json(&core.list_references(page.as_deref())?)
        }
        "page-delete" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("page-delete requires <page>"))?;
            args.remove(0);
            let mode = take_flag_value(&mut args, "page-delete", "--mode")?
                .unwrap_or_else(|| "tombstone".to_string());
            reject_extra_args("page-delete", &args)?;
            let mut receipt = serde_json::to_value(core.delete_page(&page, &mode)?)?;
            attach_delete_link_repair_lifecycle(&mut receipt);
            print_json(&receipt)
        }
        "page-restore" => {
            let page = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("page-restore requires <page>"))?;
            args.remove(0);
            reject_extra_args("page-restore", &args)?;
            print_json(&core.restore_page(&page)?)
        }
        "publish-status" => {
            reject_extra_args("publish-status", &args)?;
            print_json(&core.publish_status()?)
        }
        "publish" => {
            let wiki_engine =
                take_flag_value(&mut args, "publish", "--wiki-engine")?.map(PathBuf::from);
            let node = take_flag_value(&mut args, "publish", "--node")?
                .unwrap_or_else(|| "node".to_string());
            let trigger = take_flag_value(&mut args, "publish", "--trigger")?
                .unwrap_or_else(|| "manual".to_string());
            let force = take_bool_flag(&mut args, "--force");
            reject_extra_args("publish", &args)?;
            let result = publish(&core, &root, wiki_engine.as_deref(), &node, &trigger, force)?;
            print_json(&result)?;
            if result
                .get("status")
                .and_then(|value| value.as_str())
                .is_some_and(|status| status == "failed")
            {
                std::process::exit(2);
            }
            Ok(())
        }
        "agent-list" => {
            reject_extra_args("agent-list", &args)?;
            let store = AgentMailStore::new(root.join("context-engine"));
            let agents = latest_agent_records_for_cli(&store)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.list",
                "agent_count": agents.len(),
                "agents": agents,
            }))
        }
        "agent-whoami" => {
            let agent_id = take_flag_value(&mut args, "agent-whoami", "--agent-id")?
                .or_else(|| env::var("ONECONTEXT_AGENT_ID").ok());
            let thread_id = take_flag_value(&mut args, "agent-whoami", "--thread-id")?
                .or_else(|| env::var("ONECONTEXT_THREAD_ID").ok())
                .or_else(|| env::var("CODEX_THREAD_ID").ok());
            reject_extra_args("agent-whoami", &args)?;
            let store = AgentMailStore::new(root.join("context-engine"));
            let agent = if let Some(agent_id) = agent_id {
                store.agent_status(&agent_id)?.record
            } else if let Some(thread_id) = thread_id {
                latest_agent_records_for_cli(&store)?
                    .into_iter()
                    .find(|agent| agent.transport.thread_id == thread_id)
                    .ok_or_else(|| anyhow!("unknown agent for thread id {thread_id}"))?
            } else {
                return Err(anyhow!(
                    "agent-whoami requires --agent-id, --thread-id, ONECONTEXT_AGENT_ID, ONECONTEXT_THREAD_ID, or CODEX_THREAD_ID"
                ));
            };
            let status = store.agent_status(&agent.agent_id)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.whoami",
                "agent": status.record,
                "latest_lease": status.latest_lease,
            }))
        }
        "agent-identify" => {
            let thread_id = take_flag_value(&mut args, "agent-identify", "--thread-id")?
                .ok_or_else(|| anyhow!("agent-identify requires --thread-id"))?;
            let roles = take_repeated_flag_values(&mut args, "agent-identify", "--role")?;
            let capabilities =
                take_repeated_flag_values(&mut args, "agent-identify", "--capability")?;
            let ttl_seconds =
                take_i64_flag(&mut args, "agent-identify", "--ttl-seconds")?.unwrap_or(3600);
            if ttl_seconds <= 0 {
                return Err(anyhow!(
                    "agent-identify requires --ttl-seconds to be a positive integer"
                ));
            }
            reject_extra_args("agent-identify", &args)?;
            let now = now_rfc3339();
            let lease_expires_at = (Utc::now() + Duration::seconds(ttl_seconds))
                .to_rfc3339_opts(SecondsFormat::Secs, true);
            let policy = AgentGrantPolicy {
                allowed_roles: roles.iter().cloned().collect(),
                allowed_capabilities: capabilities.iter().cloned().collect(),
            };
            let record = AgentMailStore::new(root.join("context-engine")).identify_agent(
                &AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id,
                    requested_roles: roles,
                    requested_capabilities: capabilities,
                    lease_expires_at,
                    occurred_at: now,
                },
                &policy,
            )?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.identify",
                "agent": record,
            }))
        }
        "agent-heartbeat" => {
            let agent_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("agent-heartbeat requires <agent-id>"))?;
            args.remove(0);
            let ttl_seconds =
                take_i64_flag(&mut args, "agent-heartbeat", "--ttl-seconds")?.unwrap_or(3600);
            if ttl_seconds <= 0 {
                return Err(anyhow!(
                    "agent-heartbeat requires --ttl-seconds to be a positive integer"
                ));
            }
            reject_extra_args("agent-heartbeat", &args)?;
            let now = now_rfc3339();
            let lease_expires_at = (Utc::now() + Duration::seconds(ttl_seconds))
                .to_rfc3339_opts(SecondsFormat::Secs, true);
            let lease = AgentMailStore::new(root.join("context-engine")).heartbeat_agent(
                &agent_id,
                &lease_expires_at,
                &now,
            )?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.heartbeat",
                "agent_id": agent_id,
                "lease": lease,
            }))
        }
        "agent-retire" => {
            let agent_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("agent-retire requires <agent-id>"))?;
            args.remove(0);
            reject_extra_args("agent-retire", &args)?;
            let now = now_rfc3339();
            let agent = AgentMailStore::new(root.join("context-engine"))
                .retire_agent(&agent_id, &now, &now)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.retire",
                "agent": agent,
            }))
        }
        "agent-status" => {
            let agent_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("agent-status requires <agent-id>"))?;
            args.remove(0);
            reject_extra_args("agent-status", &args)?;
            let status =
                AgentMailStore::new(root.join("context-engine")).agent_status(&agent_id)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.status",
                "agent": status.record,
                "latest_lease": status.latest_lease,
            }))
        }
        "agent-status-by-thread" => {
            let flagged_thread_id =
                take_flag_value(&mut args, "agent-status-by-thread", "--thread-id")?;
            let thread_id = if let Some(thread_id) = flagged_thread_id {
                thread_id
            } else {
                let thread_id = args.first().cloned().ok_or_else(|| {
                    anyhow!("agent-status-by-thread requires <thread-id> or --thread-id")
                })?;
                args.remove(0);
                thread_id
            };
            reject_extra_args("agent-status-by-thread", &args)?;
            let snapshot = AgentMailStore::new(root.join("context-engine"))
                .agent_status_by_thread(&thread_id, &now_rfc3339())?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.status_by_thread",
                "thread_id": snapshot.thread_id,
                "agent_id": snapshot.agent_id,
                "lease_state": snapshot.lease_state,
                "agent": snapshot.agent,
                "latest_lease": snapshot.latest_lease,
                "active_delivery": snapshot.active_delivery,
                "pending_notification_count": snapshot.pending_notifications.len(),
                "pending_notifications": snapshot.pending_notifications,
            }))
        }
        "agent-inbox" => {
            let agent_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("agent-inbox requires <agent-id>"))?;
            args.remove(0);
            reject_extra_args("agent-inbox", &args)?;
            let rows = AgentMailStore::new(root.join("context-engine")).agent_inbox(&agent_id)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.agent.inbox",
                "agent_id": agent_id,
                "message_count": rows.len(),
                "deliveries": rows,
            }))
        }
        "mail-send" => {
            let from = take_flag_value(&mut args, "mail-send", "--from")?
                .ok_or_else(|| anyhow!("mail-send requires --from"))?;
            let to = take_repeated_flag_values(&mut args, "mail-send", "--to")?;
            if to.is_empty() {
                return Err(anyhow!("mail-send requires at least one --to"));
            }
            let cc = take_repeated_flag_values(&mut args, "mail-send", "--cc")?;
            let kind = take_flag_value(&mut args, "mail-send", "--kind")?
                .unwrap_or_else(|| "message".to_string());
            let subject = take_flag_value(&mut args, "mail-send", "--subject")?
                .ok_or_else(|| anyhow!("mail-send requires --subject"))?;
            let body_markdown = read_text_arg(&mut args, "mail-send", "--body", "--body-file")?
                .ok_or_else(|| anyhow!("mail-send requires --body or --body-file"))?;
            let body_sha256 = take_flag_value(&mut args, "mail-send", "--body-sha256")?
                .unwrap_or_else(|| sha256_hex(body_markdown.as_bytes()));
            let idempotency_key = take_flag_value(&mut args, "mail-send", "--idempotency-key")?
                .unwrap_or_else(|| {
                    stable_cli_id(
                        "mailkey",
                        &format!(
                            "{from}\n{}\n{}\n{kind}\n{subject}\n{body_sha256}",
                            to.join("\n"),
                            cc.join("\n")
                        ),
                    )
                });
            let created_at = take_flag_value(&mut args, "mail-send", "--created-at")?
                .unwrap_or_else(now_rfc3339);
            let message_id = take_flag_value(&mut args, "mail-send", "--message-id")?
                .unwrap_or_else(|| stable_cli_id("mailmsg", &idempotency_key));
            let thread_id = take_flag_value(&mut args, "mail-send", "--thread-id")?
                .unwrap_or_else(|| stable_cli_id("thread", &idempotency_key));
            let reply_to = take_flag_value(&mut args, "mail-send", "--reply-to")?;
            let page_id = take_flag_value(&mut args, "mail-send", "--page-id")?;
            let page_route = take_flag_value(&mut args, "mail-send", "--page-route")?;
            let attachment_refs =
                take_repeated_flag_values(&mut args, "mail-send", "--attachment-ref-json")?;
            let max_open_deliveries_per_recipient = take_i64_flag(
                &mut args,
                "mail-send",
                "--max-open-deliveries-per-recipient",
            )?;
            if max_open_deliveries_per_recipient.is_some_and(|limit| limit < 0) {
                return Err(anyhow!(
                    "mail-send requires --max-open-deliveries-per-recipient to be zero or greater"
                ));
            }
            reject_extra_args("mail-send", &args)?;

            let page = match (page_id, page_route) {
                (Some(id), Some(route)) => Some(MessagePageRef { id, route }),
                (None, None) => None,
                _ => {
                    return Err(anyhow!(
                        "mail-send requires --page-id and --page-route together"
                    ))
                }
            };
            let attachments = parse_attachment_refs(attachment_refs)?;
            let envelope = MessageEnvelope {
                schema_version: 1,
                message_id,
                idempotency_key,
                kind,
                subject,
                from,
                to,
                cc,
                page,
                thread_id,
                reply_to,
                body: MessageBodyRef {
                    format: "markdown".to_string(),
                    sha256: body_sha256,
                },
                attachments,
                created_at,
            };
            let options = SendMailOptions {
                max_open_deliveries_per_recipient: max_open_deliveries_per_recipient
                    .map(|limit| limit as usize),
            };
            let receipt = AgentMailStore::new(root.join("context-engine")).send_mail(
                &envelope,
                &body_markdown,
                &options,
            )?;
            let acceptance = message_acceptance_value(&receipt.acceptance);
            let attempts = receipt
                .attempts
                .into_iter()
                .map(|attempt| {
                    json!({
                        "recipient": attempt.recipient,
                        "delivery_id": attempt.delivery_id,
                        "status": delivery_attempt_status_name(&attempt.status),
                    })
                })
                .collect::<Vec<_>>();
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.send",
                "message": envelope,
                "acceptance": acceptance,
                "delivery_attempt_count": attempts.len(),
                "delivery_attempts": attempts,
                "next_action": "wiki.notify.dispatch",
                "repair_hints": [],
            }))
        }
        "mail-inbox" => {
            let address = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail-inbox requires <address>"))?;
            args.remove(0);
            reject_extra_args("mail-inbox", &args)?;
            let address = MailAddress::parse(&address)?;
            let rows = AgentMailStore::new(root.join("context-engine")).mail_inbox(&address)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.inbox",
                "recipient": address.canonical(),
                "message_count": rows.len(),
                "deliveries": rows,
            }))
        }
        "mail-read" => {
            let message_id = take_flag_value(&mut args, "mail-read", "--message-id")?;
            let thread_id = take_flag_value(&mut args, "mail-read", "--thread-id")?;
            reject_extra_args("mail-read", &args)?;
            let store = AgentMailStore::new(root.join("context-engine"));
            match (message_id, thread_id) {
                (Some(message_id), None) => {
                    let message = store.read_message(&message_id)?;
                    print_json(&json!({
                        "schema_version": 1,
                        "status": "ok",
                        "operation": "wiki.mail.read",
                        "message": message,
                    }))
                }
                (None, Some(thread_id)) => {
                    let messages = store.read_thread(&thread_id)?;
                    print_json(&json!({
                        "schema_version": 1,
                        "status": "ok",
                        "operation": "wiki.mail.read",
                        "thread_id": thread_id,
                        "message_count": messages.len(),
                        "messages": messages,
                    }))
                }
                (Some(_), Some(_)) => Err(anyhow!(
                    "mail-read accepts either --message-id or --thread-id, not both"
                )),
                (None, None) => Err(anyhow!("mail-read requires --message-id or --thread-id")),
            }
        }
        "mail-open" => {
            let delivery_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail-open requires <delivery-id>"))?;
            args.remove(0);
            let agent_id = take_flag_value(&mut args, "mail-open", "--agent-id")?
                .ok_or_else(|| anyhow!("mail-open requires --agent-id"))?;
            reject_extra_args("mail-open", &args)?;
            let opened = AgentMailStore::new(root.join("context-engine"))
                .open_delivery(&delivery_id, &agent_id)?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.open",
                "delivery": opened.delivery,
                "message": opened.message,
                "content_delivery": opened.content_delivery,
            }))
        }
        "mail-record-injection" => {
            let delivery_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail-record-injection requires <delivery-id>"))?;
            args.remove(0);
            let agent_id = take_flag_value(&mut args, "mail-record-injection", "--agent-id")?
                .ok_or_else(|| anyhow!("mail-record-injection requires --agent-id"))?;
            let result = parse_mail_injection_result(
                take_flag_value(&mut args, "mail-record-injection", "--result")?
                    .unwrap_or_else(|| "ok".to_string())
                    .as_str(),
            )?;
            let thread_id = take_flag_value(&mut args, "mail-record-injection", "--thread-id")?;
            let item_count =
                take_i64_flag(&mut args, "mail-record-injection", "--item-count")?.unwrap_or(1);
            if item_count < 0 {
                return Err(anyhow!(
                    "mail-record-injection requires --item-count to be non-negative"
                ));
            }
            let error = take_flag_value(&mut args, "mail-record-injection", "--error")?;
            reject_extra_args("mail-record-injection", &args)?;
            let recorded = AgentMailStore::new(root.join("context-engine"))
                .record_mail_injection_result(
                    &delivery_id,
                    &agent_id,
                    thread_id.as_deref(),
                    item_count as usize,
                    result,
                    &now_rfc3339(),
                    error,
                )?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.record_injection",
                "receipt": recorded.receipt,
                "control_event": recorded.control_event,
            }))
        }
        "mail-claim" => {
            let delivery_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail-claim requires <delivery-id>"))?;
            args.remove(0);
            let agent_id = take_flag_value(&mut args, "mail-claim", "--agent-id")?
                .ok_or_else(|| anyhow!("mail-claim requires --agent-id"))?;
            reject_extra_args("mail-claim", &args)?;
            let delivery = AgentMailStore::new(root.join("context-engine")).claim_delivery(
                &delivery_id,
                &agent_id,
                &now_rfc3339(),
            )?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.claim",
                "delivery": delivery,
            }))
        }
        "mail-mark" => {
            let delivery_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail-mark requires <delivery-id>"))?;
            args.remove(0);
            let agent_id = take_flag_value(&mut args, "mail-mark", "--agent-id")?
                .ok_or_else(|| anyhow!("mail-mark requires --agent-id"))?;
            let state = parse_delivery_state(
                take_flag_value(&mut args, "mail-mark", "--state")?
                    .ok_or_else(|| anyhow!("mail-mark requires --state"))?
                    .as_str(),
            )?;
            reject_extra_args("mail-mark", &args)?;
            let delivery = AgentMailStore::new(root.join("context-engine")).mark_delivery(
                &delivery_id,
                &agent_id,
                state,
                &now_rfc3339(),
            )?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.mark",
                "delivery": delivery,
            }))
        }
        "mail-mark-all" => {
            let agent_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail-mark-all requires <agent-id>"))?;
            args.remove(0);
            let state = parse_delivery_state(
                take_flag_value(&mut args, "mail-mark-all", "--state")?
                    .ok_or_else(|| anyhow!("mail-mark-all requires --state"))?
                    .as_str(),
            )?;
            let recipient = take_flag_value(&mut args, "mail-mark-all", "--recipient")?;
            let dry_run = take_bool_flag(&mut args, "--dry-run");
            reject_extra_args("mail-mark-all", &args)?;
            let store = AgentMailStore::new(root.join("context-engine"));
            let rows = store.agent_inbox(&agent_id)?;
            let selected = rows
                .into_iter()
                .filter(|row| row.state != state)
                .filter(|row| !row.state.is_terminal_for_cli())
                .filter(|row| {
                    recipient
                        .as_ref()
                        .map(|recipient| &row.recipient == recipient)
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>();
            let mut deliveries = Vec::new();
            if !dry_run {
                let occurred_at = now_rfc3339();
                for row in &selected {
                    deliveries.push(store.mark_delivery(
                        &row.delivery_id,
                        &agent_id,
                        state.clone(),
                        &occurred_at,
                    )?);
                }
            }
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.mark_all",
                "agent_id": agent_id,
                "state": state,
                "dry_run": dry_run,
                "selected_count": selected.len(),
                "selected_delivery_ids": selected.iter().map(|row| row.delivery_id.clone()).collect::<Vec<_>>(),
                "updated_count": deliveries.len(),
                "deliveries": deliveries,
            }))
        }
        "mail-snooze" => {
            let delivery_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail-snooze requires <delivery-id>"))?;
            args.remove(0);
            let agent_id = take_flag_value(&mut args, "mail-snooze", "--agent-id")?
                .ok_or_else(|| anyhow!("mail-snooze requires --agent-id"))?;
            let until = take_flag_value(&mut args, "mail-snooze", "--until")?
                .ok_or_else(|| anyhow!("mail-snooze requires --until"))?;
            reject_extra_args("mail-snooze", &args)?;
            let delivery = AgentMailStore::new(root.join("context-engine")).snooze_delivery(
                &delivery_id,
                &agent_id,
                &until,
                &now_rfc3339(),
            )?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.mail.snooze",
                "delivery": delivery,
            }))
        }
        "notify-poll" => {
            let agent_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("notify-poll requires <agent-id>"))?;
            args.remove(0);
            let cursor = take_flag_value(&mut args, "notify-poll", "--cursor")?;
            reject_extra_args("notify-poll", &args)?;
            let notifications = AgentMailStore::new(root.join("context-engine"))
                .notification_poll(&agent_id, cursor.as_deref(), &now_rfc3339())?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.notify.poll",
                "agent_id": agent_id,
                "notification_count": notifications.len(),
                "notifications": notifications,
            }))
        }
        "notify-ack" => {
            let notification_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("notify-ack requires <notification-id>"))?;
            args.remove(0);
            let agent_id = take_flag_value(&mut args, "notify-ack", "--agent-id")?
                .ok_or_else(|| anyhow!("notify-ack requires --agent-id"))?;
            reject_extra_args("notify-ack", &args)?;
            let notification = AgentMailStore::new(root.join("context-engine"))
                .acknowledge_notification(&agent_id, &notification_id, &now_rfc3339())?;
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.notify.ack",
                "notification": notification,
            }))
        }
        "notify-dispatch" => {
            let agent_id = args
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("notify-dispatch requires <agent-id>"))?;
            args.remove(0);
            let dry_run = take_bool_flag(&mut args, "--dry-run");
            let steering_command =
                take_flag_value(&mut args, "notify-dispatch", "--steering-command")?;
            let steering_args =
                take_repeated_flag_values(&mut args, "notify-dispatch", "--steering-arg")?;
            let payload_format = take_flag_value(&mut args, "notify-dispatch", "--payload-format")?
                .unwrap_or_else(|| "text".to_string());
            let limit = take_i64_flag(&mut args, "notify-dispatch", "--limit")?
                .unwrap_or(25)
                .max(0) as usize;
            reject_extra_args("notify-dispatch", &args)?;
            let store = AgentMailStore::new(root.join("context-engine"));
            let notifications =
                store.notification_dispatch_queue(&agent_id, None, &now_rfc3339())?;
            let mut attempts = Vec::new();
            let mut dispatch_decisions = Vec::new();
            for notification in notifications.into_iter().take(limit) {
                let payload = store.codex_steering_payload(&notification)?;
                let steering_text = store.codex_steering_text(&payload);
                let occurred_at = now_rfc3339();
                let decision = decide_codex_notification_dispatch(CodexDispatchRequest {
                    payload: &payload,
                    runtime_status: CodexRuntimeStatus::ActiveTurn {
                        thread_id: payload.thread_id.clone(),
                        turn_id: None,
                    },
                    policy: CodexSupervisorPolicy::default(),
                })?;
                let (status, error) = if !decision.is_dispatchable() {
                    (
                        NotificationAttemptStatus::Failed,
                        Some(serde_json::to_string(&decision.evidence.terminal_error)?),
                    )
                } else if dry_run {
                    (NotificationAttemptStatus::DryRun, None)
                } else {
                    let command = steering_command.as_ref().ok_or_else(|| {
                        anyhow!("notify-dispatch requires --dry-run or --steering-command")
                    })?;
                    match send_steering_command(
                        command,
                        &steering_args,
                        &payload_format,
                        &payload,
                        &steering_text,
                    ) {
                        Ok(()) => (NotificationAttemptStatus::Sent, None),
                        Err(error) => (
                            NotificationAttemptStatus::Failed,
                            Some(format!("{error:#}")),
                        ),
                    }
                };
                dispatch_decisions.push(decision);
                attempts.push(store.record_notification_attempt(
                    &notification,
                    status,
                    &occurred_at,
                    error,
                )?);
            }
            print_json(&json!({
                "schema_version": 1,
                "status": "ok",
                "operation": "wiki.notify.dispatch",
                "agent_id": agent_id,
                "attempt_count": attempts.len(),
                "attempts": attempts,
                "dispatch_decisions": dispatch_decisions,
            }))
        }
        "talk-append" => {
            let page = take_flag_value(&mut args, "talk-append", "--page")?
                .ok_or_else(|| anyhow!("talk-append requires --page"))?;
            let kind = take_flag_value(&mut args, "talk-append", "--kind")?
                .unwrap_or_else(|| "proposal".to_string());
            let subject = take_flag_value(&mut args, "talk-append", "--subject")?
                .ok_or_else(|| anyhow!("talk-append requires --subject"))?;
            let thread_id = take_flag_value(&mut args, "talk-append", "--thread-id")?;
            let reply_to = take_flag_value(&mut args, "talk-append", "--reply-to")?;
            let operation_id = take_flag_value(&mut args, "talk-append", "--operation-id")?;
            let delivery_mode = parse_talk_delivery_mode(take_flag_value(
                &mut args,
                "talk-append",
                "--delivery-mode",
            )?)?;
            let from = take_flag_value(&mut args, "talk-append", "--from")?
                .ok_or_else(|| anyhow!("talk-append requires --from"))?;
            let body_markdown =
                read_text_arg(&mut args, "talk-append", "--body", "--body-file")?
                    .ok_or_else(|| anyhow!("talk-append requires --body or --body-file"))?;
            let to = take_repeated_flag_values(&mut args, "talk-append", "--to")?;
            let cc = take_repeated_flag_values(&mut args, "talk-append", "--cc")?;
            let attachments = talk_attachment_inputs(
                take_repeated_flag_values(&mut args, "talk-append", "--attachment")?,
                take_repeated_flag_values(&mut args, "talk-append", "--attachment-filename")?,
                take_repeated_flag_values(&mut args, "talk-append", "--attachment-caption")?,
                take_repeated_flag_values(&mut args, "talk-append", "--attachment-alt")?,
            )?;
            let allow_tombstoned = take_bool_flag(&mut args, "--allow-tombstoned");
            reject_extra_args("talk-append", &args)?;
            print_json(&core.append_talk(TalkAppendRequest {
                page,
                kind,
                subject,
                operation_id,
                delivery_mode,
                thread_id,
                reply_to,
                from,
                to,
                cc,
                body_markdown,
                attachments,
                allow_tombstoned,
            })?)
        }
        _ => return Err(anyhow!("unknown command: {command}")),
    }
}

fn parse_talk_delivery_mode(value: Option<String>) -> Result<TalkDeliveryMode> {
    match value.as_deref() {
        None | Some("labels-only") | Some("labels_only") => Ok(TalkDeliveryMode::LabelsOnly),
        Some("mail") => Ok(TalkDeliveryMode::Mail),
        Some(other) => Err(anyhow!(
            "invalid talk-append --delivery-mode {other:?}; expected labels-only or mail"
        )),
    }
}

fn parse_delivery_state(value: &str) -> Result<DeliveryState> {
    match value {
        "read" => Ok(DeliveryState::Read),
        "done" => Ok(DeliveryState::Done),
        "archived" => Ok(DeliveryState::Archived),
        "rejected" => Ok(DeliveryState::Rejected),
        other => Err(anyhow!(
            "invalid mail-mark --state {other:?}; expected read, done, archived, or rejected"
        )),
    }
}

trait DeliveryStateCliExt {
    fn is_terminal_for_cli(&self) -> bool;
}

impl DeliveryStateCliExt for DeliveryState {
    fn is_terminal_for_cli(&self) -> bool {
        matches!(
            self,
            Self::Done | Self::Archived | Self::Rejected | Self::DeadLetter
        )
    }
}

fn parse_attachment_refs(values: Vec<String>) -> Result<Vec<MessageAttachmentRef>> {
    values
        .into_iter()
        .map(|value| {
            serde_json::from_str::<MessageAttachmentRef>(&value)
                .with_context(|| "parse --attachment-ref-json as MessageAttachmentRef")
        })
        .collect()
}

fn latest_agent_records_for_cli(store: &AgentMailStore) -> Result<Vec<AgentRecord>> {
    let mut latest = BTreeMap::new();
    for record in read_jsonl_for_cli::<AgentRecord>(&store.paths().agents_path())? {
        latest.insert(record.agent_id.clone(), record);
    }
    Ok(latest.into_values().collect())
}

fn read_jsonl_for_cli<T>(path: &Path) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("open {}", path.display())),
    };
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(index, line)| match line {
            Ok(line) if line.trim().is_empty() => None,
            Ok(line) => Some(
                serde_json::from_str::<T>(&line)
                    .with_context(|| format!("parse {} line {}", path.display(), index + 1)),
            ),
            Err(error) => Some(Err(error).with_context(|| format!("read {}", path.display()))),
        })
        .collect()
}

fn message_acceptance_value(acceptance: &MessageAcceptance) -> serde_json::Value {
    match acceptance {
        MessageAcceptance::Accepted => json!({
            "status": "accepted",
        }),
        MessageAcceptance::DuplicateSamePayload { message_id } => json!({
            "status": "duplicate_same_payload",
            "message_id": message_id,
        }),
    }
}

fn delivery_attempt_status_name(status: &DeliveryAttemptStatus) -> &'static str {
    match status {
        DeliveryAttemptStatus::Delivered => "delivered",
        DeliveryAttemptStatus::AlreadyDelivered => "already_delivered",
        DeliveryAttemptStatus::DeferredCapacity => "deferred_capacity",
    }
}

fn parse_mail_injection_result(value: &str) -> Result<MailInjectionResult> {
    match value {
        "ok" => Ok(MailInjectionResult::Ok),
        "failed" => Ok(MailInjectionResult::Failed),
        other => Err(anyhow!(
            "invalid mail injection result {other:?}; expected ok or failed"
        )),
    }
}

fn stable_cli_id(prefix: &str, input: &str) -> String {
    format!("{prefix}_{:016x}", fnv1a64(input.as_bytes()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn sha256_hex(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64) * 8;
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0_u32; 64];
        for (index, word) in w.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|word| format!("{word:08x}")).collect()
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn send_steering_command(
    command: &str,
    args: &[String],
    payload_format: &str,
    payload: &CodexSteeringPayload,
    steering_text: &str,
) -> Result<()> {
    let input = match payload_format {
        "text" => steering_text.to_string(),
        "json" => serde_json::to_string(&json!({
            "payload": payload,
            "steering_text": steering_text,
        }))?,
        other => {
            return Err(anyhow!(
                "invalid notify-dispatch --payload-format {other:?}; expected text or json"
            ))
        }
    };
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn steering command {command}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow!("steering command stdin unavailable"))?
        .write_all(input.as_bytes())
        .with_context(|| format!("write steering payload to {command}"))?;
    let output = child
        .wait_with_output()
        .with_context(|| format!("wait for steering command {command}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(anyhow!(
        "steering command {command} failed with status {}; stderr={:?}; stdout={:?}",
        output.status,
        stderr,
        stdout
    ))
}

fn take_flag_value(args: &mut Vec<String>, command: &str, flag: &str) -> Result<Option<String>> {
    let Some(index) = args.iter().position(|arg| arg == flag) else {
        return Ok(None);
    };
    args.remove(index);
    let Some(value) = args.get(index) else {
        return Err(anyhow!("{command} requires a value for {flag}"));
    };
    if value.starts_with("--") {
        return Err(anyhow!("{command} requires a value for {flag}"));
    }
    Ok(Some(args.remove(index)))
}

fn take_repeated_flag_values(
    args: &mut Vec<String>,
    command: &str,
    flag: &str,
) -> Result<Vec<String>> {
    let mut values = Vec::new();
    while let Some(value) = take_flag_value(args, command, flag)? {
        values.push(value);
    }
    Ok(values)
}

fn take_bool_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let present = args.iter().any(|arg| arg == flag);
    args.retain(|arg| arg != flag);
    present
}

fn take_i64_flag(args: &mut Vec<String>, command: &str, flag: &str) -> Result<Option<i64>> {
    let Some(value) = take_flag_value(args, command, flag)? else {
        return Ok(None);
    };
    value.parse::<i64>().map(Some).map_err(|_| {
        anyhow!("invalid numeric flag {flag} for {command}: {value:?}; expected an integer")
    })
}

fn reject_extra_args(command: &str, args: &[String]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "{command} received unexpected argument(s): {}",
            args.join(" ")
        ))
    }
}

fn read_text_arg(
    args: &mut Vec<String>,
    command: &str,
    value_flag: &str,
    file_flag: &str,
) -> Result<Option<String>> {
    let value = take_flag_value(args, command, value_flag)?;
    let path = take_flag_value(args, command, file_flag)?;
    match (value, path) {
        (Some(_), Some(_)) => Err(anyhow!(
            "{command} accepts either {value_flag} or {file_flag}, not both"
        )),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) => fs::read_to_string(&path)
            .map(Some)
            .with_context(|| format!("read {path}")),
        (None, None) => Ok(None),
    }
}

fn talk_attachment_inputs(
    paths: Vec<String>,
    filenames: Vec<String>,
    captions: Vec<String>,
    alt_texts: Vec<String>,
) -> Result<Vec<TalkAttachmentInput>> {
    for (flag, values) in [
        ("--attachment-filename", &filenames),
        ("--attachment-caption", &captions),
        ("--attachment-alt", &alt_texts),
    ] {
        if values.len() > paths.len() {
            return Err(anyhow!(
                "talk-append received {flag} metadata without a matching --attachment"
            ));
        }
    }

    Ok(paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| TalkAttachmentInput {
            path,
            filename: filenames.get(index).cloned(),
            caption: captions.get(index).cloned(),
            alt_text: alt_texts.get(index).cloned(),
        })
        .collect())
}

fn command_name(args: &[String]) -> Option<&str> {
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--root" {
            skip_next = true;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        return Some(arg);
    }
    None
}

fn operation_for_command(command: &str) -> &'static str {
    match canonical_command(command).as_str() {
        "ensure" => "wiki.ensure",
        "status" => "wiki.status",
        "validate" => "wiki.validate",
        "list" => "wiki.list",
        "page-status" => "wiki.page.status",
        "page-open" => "wiki.page.open",
        "page-create" => "wiki.page.create",
        "page-create-all" => "wiki.page.create_all",
        "page-write-body" => "wiki.page.write_body",
        "page-patch-body" => "wiki.page.patch_body",
        "asset-add" => "wiki.asset.add",
        "asset-list" => "wiki.asset.list",
        "reference-list" => "wiki.reference.list",
        "page-delete" => "wiki.page.delete",
        "page-restore" => "wiki.page.restore",
        "publish-status" => "wiki.publish.status",
        "publish" => "wiki.publish",
        "agent-list" => "wiki.agent.list",
        "agent-whoami" => "wiki.agent.whoami",
        "agent-identify" => "wiki.agent.identify",
        "agent-heartbeat" => "wiki.agent.heartbeat",
        "agent-retire" => "wiki.agent.retire",
        "agent-status" => "wiki.agent.status",
        "agent-status-by-thread" => "wiki.agent.status_by_thread",
        "agent-inbox" => "wiki.agent.inbox",
        "mail-send" => "wiki.mail.send",
        "mail-inbox" => "wiki.mail.inbox",
        "mail-read" => "wiki.mail.read",
        "mail-open" => "wiki.mail.open",
        "mail-record-injection" => "wiki.mail.record_injection",
        "mail-claim" => "wiki.mail.claim",
        "mail-mark" => "wiki.mail.mark",
        "mail-mark-all" => "wiki.mail.mark_all",
        "mail-snooze" => "wiki.mail.snooze",
        "notify-poll" => "wiki.notify.poll",
        "notify-ack" => "wiki.notify.ack",
        "notify-dispatch" => "wiki.notify.dispatch",
        "talk-append" => "wiki.talk.append",
        _ => "wiki.unknown",
    }
}

fn canonical_command(command: &str) -> String {
    let normalized = command.replace(['.', '_'], "-");
    let stripped = normalized.strip_prefix("wiki-").unwrap_or(&normalized);
    match stripped {
        "page-asset-add" => "asset-add".to_string(),
        "page-asset-list" => "asset-list".to_string(),
        "references" => "reference-list".to_string(),
        "agent-identity" => "agent-whoami".to_string(),
        "agent-status-by-thread-id" => "agent-status-by-thread".to_string(),
        "mail-injection-record" => "mail-record-injection".to_string(),
        other => other.to_string(),
    }
}

fn error_code(message: &str) -> &'static str {
    if message.contains("snooze until") || message.contains("state snoozed requires --until") {
        "invalid_snooze_until"
    } else if message.contains("timed out waiting for") && message.contains(" lock at ") {
        "mutation_lock_busy"
    } else if message.contains("requires ")
        || message.contains("accepts either")
        || message.contains("metadata without a matching --attachment")
    {
        "invalid_arguments"
    } else if message.contains("invalid numeric flag") {
        "invalid_arguments"
    } else if message.contains("invalid page route") {
        "invalid_page_route"
    } else if message.contains("invalid page slug") {
        "invalid_page_slug"
    } else if message.contains("invalid page family_group")
        || message.contains("invalid page family_id")
    {
        "invalid_page_path"
    } else if message.contains("invalid nav section") {
        "invalid_nav_section"
    } else if message.contains("page source path already exists") {
        "page_source_path_conflict"
    } else if message.contains("user-wiki/wiki.toml") && message.contains("TOML parse error") {
        "invalid_wiki_config"
    } else if message.contains("asset file not found")
        || message.contains("asset filename missing")
        || message.contains("image asset requires")
        || message.contains("file asset requires")
    {
        "invalid_asset"
    } else if message.contains("attachment not found")
        || message.contains("attachment filename missing")
        || message.contains("invalid attachment filename")
    {
        "invalid_attachment"
    } else if message.contains("page-create refused for tombstoned page")
        || message.contains("talk append refused for tombstoned page")
        || message.contains("page body edit refused for tombstoned page")
    {
        "tombstoned_page"
    } else if message.contains("page-create refused for disabled page")
        || message.contains("talk append refused for disabled page")
        || message.contains("page body edit refused for disabled page")
    {
        "disabled_page"
    } else if message.contains("page already exists:") {
        "page_already_exists"
    } else if message.contains("route already exists:") {
        "route_already_exists"
    } else if message.contains("generated site page") && message.contains("not source-backed") {
        "generated_site_page"
    } else if message.contains("unknown page") || message.contains("unknown configured page") {
        "unknown_page"
    } else if message.contains("unexpected argument") {
        "unexpected_arguments"
    } else if message.contains("source hash mismatch") {
        "source_hash_mismatch"
    } else if message.contains("patch text not found") {
        "body_patch_not_found"
    } else if message.contains("patch text matched") {
        "body_patch_ambiguous"
    } else if message.contains("template path escapes templates/")
        || message.contains("read template")
        || message.contains("missing required frontmatter field")
        || message.contains("invalid frontmatter section")
        || message.contains("invalid frontmatter access")
    {
        "invalid_page_template"
    } else if message.contains("not found") {
        "not_found"
    } else if message.contains("missing renderer") {
        "missing_renderer"
    } else {
        "command_failed"
    }
}

fn error_details(message: &str) -> Option<serde_json::Value> {
    if let Some(rest) = message.strip_prefix("source hash mismatch for ") {
        let (page, rest) = rest.split_once("; expected ")?;
        let (expected, found) = rest.split_once(", found ")?;
        return Some(json!({
            "page": page,
            "expected_source_sha256": expected.trim(),
            "found_source_sha256": found.trim(),
        }));
    }
    None
}

fn repair_hints(message: &str) -> Vec<&'static str> {
    if message.contains("snooze until") || message.contains("state snoozed requires --until") {
        vec!["Use --until with an RFC3339 timestamp in the future, for example 2099-01-01T00:00:00Z."]
    } else if message.contains("timed out waiting for") && message.contains(" lock at ") {
        vec!["Retry after the current wiki mutation finishes."]
    } else if message.contains("talk-append requires --body or --body-file")
        || message.contains("talk-append accepts either --body or --body-file, not both")
    {
        vec!["Provide exactly one talk body source: --body <markdown> for short notes, or --body-file <path> for longer notes."]
    } else if message.contains("requires ") || message.contains("accepts either") {
        vec!["Run with --help and provide the required arguments."]
    } else if message.contains("invalid numeric flag") {
        vec!["Use an integer value for numeric flags."]
    } else if message.contains("invalid page route") {
        vec!["Use an absolute extensionless route such as /projects/example. Keep segments lowercase and unique."]
    } else if message.contains("invalid page slug") {
        vec!["Use a lowercase slug with letters, numbers, hyphens, or dots; avoid path separators, uppercase letters, and traversal segments."]
    } else if message.contains("invalid page family_group")
        || message.contains("invalid page family_id")
    {
        vec!["Use safe lowercase path tokens for --family-group and --family-id. Avoid slashes, spaces, uppercase letters, and .. traversal."]
    } else if message.contains("invalid nav section") {
        vec!["Use --nav-section primary, utility, or hidden."]
    } else if message.contains("page source path already exists") {
        vec!["Choose a unique --slug or --family-id so the new page does not share another page's source/talk path."]
    } else if message.contains("attachment not found")
        || message.contains("attachment filename missing")
        || message.contains("invalid attachment filename")
    {
        vec!["Pass --attachment with an existing file path. The copied talk attachment will use a safe filename under the talk folder."]
    } else if message.contains("unknown page") || message.contains("unknown configured page") {
        vec!["Run wiki.list to inspect configured pages or create the page first."]
    } else if message.contains("user-wiki/wiki.toml") && message.contains("TOML parse error") {
        vec!["Repair user-wiki/wiki.toml before retrying. Run wiki.validate after fixing the TOML syntax."]
    } else if message.contains("page-create refused for tombstoned page") {
        vec!["Use a new page id, or wait for an explicit restore operation instead of recreating over a tombstone."]
    } else if message.contains("page-create refused for disabled page") {
        vec!["Use a new page id, or explicitly re-enable/restore the page before creating source files."]
    } else if message.contains("page already exists:") {
        vec!["Use the existing page id with wiki.page.open/status, or choose a new page id for a distinct page."]
    } else if message.contains("route already exists:") {
        vec!["Run wiki.list to find the page that owns this route. Tombstoned routes stay reserved until an explicit restore operation exists; choose a new route for a replacement page."]
    } else if message.contains("talk append refused for tombstoned page") {
        vec!["Pass --allow-tombstoned only for explicit archive-maintenance talk, or reopen/create a live page before appending normal work."]
    } else if message.contains("talk append refused for disabled page") {
        vec!["Restore or re-enable the page before appending normal talk. Use --allow-tombstoned only for explicit archive-maintenance talk."]
    } else if message.contains("page body edit refused for tombstoned page") {
        vec!["Run wiki.page.restore before editing, or choose an enabled replacement page for new content."]
    } else if message.contains("page body edit refused for disabled page") {
        vec!["Restore or re-enable the page before editing, or choose an enabled replacement page for new content."]
    } else if message.contains("generated site page") && message.contains("not source-backed") {
        vec!["Generated site pages are not editable source pages. Use wiki.publish/status or inspect user-wiki/site/.1context/route-manifest.json for their rendered routes."]
    } else if message.contains("unexpected argument") {
        vec!["Run with --help and remove unsupported trailing arguments."]
    } else if message.contains("source hash mismatch") {
        vec!["Run wiki.page.open again, read edit.expected_source_sha256, and retry with the current hash."]
    } else if message.contains("patch text not found") {
        vec!["Run wiki.page.open to inspect the current source body before patching."]
    } else if message.contains("patch text matched") {
        vec!["Make --find more specific so it matches exactly one body span, or use page-write-body with the current expected_source_sha256."]
    } else if message.contains("template path escapes templates/")
        || message.contains("read template")
    {
        vec!["Use a template path under user-wiki/templates, such as pages/context-page.md and talk/conventions/topics.md."]
    } else if message.contains("missing required frontmatter field")
        || message.contains("invalid frontmatter section")
        || message.contains("invalid frontmatter access")
    {
        vec![
            "Fix the page template frontmatter before retrying page-create. Required renderer fields are title, slug, section, and access.",
            "Allowed section values are for-you, context, project, work, reference, system, and site; allowed access values are public, shared, and private.",
        ]
    } else if message.contains("missing renderer") {
        vec!["Pass --wiki-engine pointing at a directory containing tools/render-site.mjs."]
    } else {
        Vec::new()
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn publish(
    core: &WikiCore,
    root: &Path,
    wiki_engine: Option<&Path>,
    node: &str,
    trigger: &str,
    force: bool,
) -> Result<serde_json::Value> {
    let mut preflight = Vec::new();
    let before = core.publish_status()?;
    let validation = core.validate()?;
    if !validation.can_publish {
        let next_action = validation.next_action.clone();
        let repair_hints = if validation.repair_hints.is_empty() {
            vec!["Run wiki.validate and repair blocking issues before publishing.".to_string()]
        } else {
            validation.repair_hints.clone()
        };
        return Ok(json!({
            "schema_version": 1,
            "status": "failed",
            "operation": "wiki.publish",
            "trigger": trigger,
            "preflight": preflight,
            "before": before,
            "render_required": false,
            "next_action": next_action,
            "validation": validation,
            "repair_hints": repair_hints
        }));
    }
    let mut render_input = before.clone();
    if !render_input.pages_missing_source.is_empty() || !render_input.pages_missing_talk.is_empty()
    {
        let create_all = core.create_all_pages_for_publish_preflight()?;
        preflight.push(json!({
            "operation": "wiki.publish.preflight",
            "action": "backfill_configured_pages",
            "reason": "publish_missing_configured_pages",
            "result": create_all
        }));
        render_input = core.publish_status()?;
    }
    if !force && !render_input.render_required {
        return Ok(json!({
            "schema_version": 1,
            "status": "skipped",
            "operation": "wiki.publish",
            "trigger": trigger,
            "preflight": preflight,
            "before": before,
            "render_input": render_input,
            "after": render_input,
            "render_required": false,
            "next_action": "none",
            "repair_hints": []
        }));
    }

    let engine = resolve_wiki_engine(wiki_engine)?;
    let render_tool = engine.join("tools/render-site.mjs");
    if !render_tool.is_file() {
        return Err(anyhow!("missing renderer: {}", render_tool.display()));
    }
    let runs = root.join("context-engine/runs");
    fs::create_dir_all(&runs)?;
    let result_json = runs.join("wiki-publish-result.json");
    let output = root.join("user-wiki/site");
    let staging_output = runs.join("wiki-publish-staging");
    remove_path_if_exists(&staging_output)?;
    let source_root = root.join("user-wiki/source");
    let output_result = Command::new(node)
        .arg(&render_tool)
        .arg("--source-root")
        .arg(&source_root)
        .arg("--output")
        .arg(&staging_output)
        .arg("--result-json")
        .arg(&result_json)
        .output()
        .with_context(|| format!("run renderer with {node}"))?;
    let render_result = if result_json.is_file() {
        serde_json::from_slice::<serde_json::Value>(&fs::read(&result_json)?)?
    } else {
        json!({
            "status": "failed",
            "error": String::from_utf8_lossy(&output_result.stderr).trim()
        })
    };
    let render_status = render_result
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("failed");
    let status = if output_result.status.success() && render_status == "published" {
        promote_staged_site(&staging_output, &output)?;
        let metadata = output.join(".1context");
        fs::create_dir_all(&metadata)?;
        fs::write(
            metadata.join("source-fingerprint.txt"),
            format!("{}\n", core.publish_fingerprint()?),
        )?;
        fs::write(
            metadata.join("page-fingerprints.json"),
            serde_json::to_vec_pretty(&core.page_publish_fingerprints()?)?,
        )?;
        "published"
    } else {
        "failed"
    };
    let inventory = if status == "published" {
        core.inventory().ok()
    } else {
        None
    };
    let link_diagnostics = if status == "published" {
        internal_link_diagnostics(&output, inventory.as_ref())?
    } else {
        json!({
            "status": "skipped",
            "issue_count": 0,
            "broken_internal_count": 0,
            "issues": []
        })
    };
    if status == "published" {
        annotate_link_diagnostics(&output, &link_diagnostics)?;
        annotate_route_manifest_link_diagnostics(&output, &link_diagnostics)?;
    }
    let link_issue_count = link_diagnostics
        .get("issue_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let mut after = serde_json::to_value(core.publish_status()?)?;
    if status == "published" {
        after["link_health"] = link_health_from_diagnostics(&link_diagnostics);
        if after
            .get("render_required")
            .and_then(|value| value.as_bool())
            .is_some_and(|render_required| !render_required)
        {
            after["next_action"] = if link_issue_count > 0 {
                json!("repair_links")
            } else {
                json!("none")
            };
        }
    }
    let render_required = after
        .get("render_required")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let next_action = after
        .get("next_action")
        .and_then(|value| value.as_str())
        .unwrap_or("none")
        .to_string();
    let receipt = json!({
        "schema_version": 1,
        "status": status,
        "operation": "wiki.publish",
        "trigger": trigger,
        "preflight": preflight,
        "before": before,
        "render_input": render_input,
        "after": after,
        "render_required": render_required,
        "next_action": next_action,
        "render_result_path": result_json.to_string_lossy(),
        "render_result": render_result,
        "link_diagnostics": link_diagnostics,
        "link_repair_lifecycle": link_repair_lifecycle_from_diagnostics(
            &link_diagnostics,
            Some(output.join(".1context/link-diagnostics.json").to_string_lossy().as_ref())
        ),
        "repair_hints": if status == "failed" {
            vec!["Inspect render_result.error and repair source or talk inputs before retrying wiki.publish."]
        } else if link_issue_count > 0
        {
            vec!["Inspect link_diagnostics.issues and repair stale internal links before considering the reader surface clean."]
        } else {
            Vec::<&str>::new()
        }
    });
    fs::write(
        runs.join("wiki-publish-receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

fn promote_staged_site(staging: &Path, output: &Path) -> Result<()> {
    if !staging.is_dir() {
        return Err(anyhow!(
            "renderer reported published but staged site is missing: {}",
            staging.display()
        ));
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_path_if_exists(output)?;
    fs::rename(staging, output).with_context(|| {
        format!(
            "promote staged wiki site {} to {}",
            staging.display(),
            output.display()
        )
    })?;
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)
            .with_context(|| format!("remove directory {}", path.display()))?,
        Ok(_) => fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?,
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("stat {}", path.display())),
    }
    Ok(())
}

fn attach_delete_link_repair_lifecycle(receipt: &mut serde_json::Value) {
    if let Some(lifecycle) = delete_link_repair_lifecycle(receipt) {
        receipt["link_repair_lifecycle"] = lifecycle;
    }
}

fn delete_link_repair_lifecycle(receipt: &serde_json::Value) -> Option<serde_json::Value> {
    let impact = receipt.get("link_impact")?;
    let issues = impact
        .get("issues")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let repair_tasks = link_repair_tasks_from_delete_issues(&issues);
    let inbound_link_count = impact
        .get("inbound_link_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(issues.len() as u64);
    let status = impact
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or(if inbound_link_count > 0 {
            "warning"
        } else {
            "ok"
        });
    let post_publish_next_action = impact
        .get("post_publish_expected_next_action")
        .and_then(|value| value.as_str())
        .unwrap_or(if inbound_link_count > 0 {
            "repair_links"
        } else {
            "none"
        });
    let publish_next_action = receipt
        .get("next_action")
        .and_then(|value| value.as_str())
        .unwrap_or("none");
    let publish_then_repair =
        matches!(publish_next_action, "publish" | "publish_then_repair_links")
            && post_publish_next_action == "repair_links";
    let needs_publish = matches!(publish_next_action, "publish" | "publish_then_repair_links");
    let branch = if publish_then_repair {
        link_repair_branch(
            "publish_then_repair_links",
            false,
            "wiki.publish",
            &[
                "wiki.page.open",
                "wiki.page.patch_body",
                "wiki.publish",
                "wiki.validate",
            ],
        )
    } else if needs_publish {
        link_repair_branch("publish", false, "wiki.publish", &["wiki.validate"])
    } else if post_publish_next_action == "repair_links" {
        link_repair_branch(
            "repair_links",
            false,
            "wiki.page.open",
            &["wiki.page.patch_body", "wiki.publish", "wiki.validate"],
        )
    } else {
        link_repair_branch("clean", true, "none", &[])
    };

    Some(json!({
        "status": status,
        "deleted_route": impact.get("deleted_route").cloned().unwrap_or(serde_json::Value::Null),
        "deleted_markdown_path": impact.get("deleted_markdown_path").cloned().unwrap_or(serde_json::Value::Null),
        "inbound_link_count": inbound_link_count,
        "source_page_count": impact.get("source_page_count").and_then(|value| value.as_u64()).unwrap_or(0),
        "next_action": if publish_then_repair {
            "publish_then_repair_links"
        } else if needs_publish {
            "publish"
        } else {
            post_publish_next_action
        },
        "branch": branch,
        "timeline": [
            {
                "phase": "pre_delete_preview",
                "operation": "wiki.page.delete",
                "status": status,
                "issue_count": inbound_link_count,
                "next_action": publish_next_action,
                "issues": issues
            },
            {
                "phase": "post_publish_expected_repair",
                "operation": "wiki.publish",
                "status": if inbound_link_count > 0 { "warning" } else { "ok" },
                "issue_count": inbound_link_count,
                "expected_next_action": post_publish_next_action,
                "repair_tasks": repair_tasks,
                "suggested_operations": [
                    "wiki.page.open",
                    "wiki.page.patch_body",
                    "wiki.publish",
                    "wiki.validate"
                ]
            }
        ],
        "repair_tasks": repair_tasks
    }))
}

fn link_health_from_diagnostics(diagnostics: &serde_json::Value) -> serde_json::Value {
    let issues = diagnostics
        .get("issues")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let pages_with_broken_links = issues
        .iter()
        .filter_map(|issue| issue.get("page_id").and_then(|value| value.as_str()))
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let broken_internal_count = diagnostics
        .get("issue_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    json!({
        "status": if broken_internal_count > 0 { "warning" } else { "ok" },
        "fresh": true,
        "broken_internal_count": broken_internal_count,
        "pages_with_broken_links": pages_with_broken_links,
        "repair_tasks": link_repair_tasks_from_issues(&issues)
    })
}

fn link_repair_lifecycle_from_diagnostics(
    diagnostics: &serde_json::Value,
    diagnostics_path: Option<&str>,
) -> serde_json::Value {
    let health = link_health_from_diagnostics(diagnostics);
    let issue_count = diagnostics
        .get("issue_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let repair_tasks = health
        .get("repair_tasks")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut phase = json!({
        "phase": "post_publish_link_check",
        "operation": "wiki.publish",
        "status": diagnostics.get("status").and_then(|value| value.as_str()).unwrap_or("unknown"),
        "issue_count": issue_count,
        "next_action": if issue_count > 0 { "repair_links" } else { "none" },
        "repair_tasks": repair_tasks
    });
    if let Some(path) = diagnostics_path {
        phase["diagnostics_path"] = json!(path);
    }
    json!({
        "status": health.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "broken_internal_count": health.get("broken_internal_count").cloned().unwrap_or_else(|| json!(issue_count)),
        "pages_with_broken_links": health.get("pages_with_broken_links").cloned().unwrap_or_else(|| json!([])),
        "next_action": if issue_count > 0 { "repair_links" } else { "none" },
        "branch": if issue_count > 0 {
            link_repair_branch(
                "repair_links",
                false,
                "wiki.page.open",
                &["wiki.page.patch_body", "wiki.publish", "wiki.validate"],
            )
        } else {
            link_repair_branch("clean", true, "none", &[])
        },
        "timeline": [phase],
        "repair_tasks": repair_tasks
    })
}

fn link_repair_branch(
    state: &str,
    terminal: bool,
    next_command: &str,
    followup_commands: &[&str],
) -> serde_json::Value {
    json!({
        "state": state,
        "terminal": terminal,
        "next_command": next_command,
        "followup_commands": followup_commands
    })
}

fn link_repair_tasks_from_issues(issues: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut grouped = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for issue in issues {
        if !issue
            .get("code")
            .and_then(|value| value.as_str())
            .is_some_and(|code| code == "broken_internal_link")
        {
            continue;
        }
        let page_id = issue
            .get("page_id")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let route = issue
            .get("route")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        grouped
            .entry(format!("{page_id}\0{route}"))
            .or_default()
            .push(issue.clone());
    }

    grouped
        .into_values()
        .map(|page_issues| {
            json!({
                "page_id": first_issue_string(&page_issues, "page_id").unwrap_or_else(|| "unknown".to_string()),
                "route": first_issue_string(&page_issues, "route").unwrap_or_default(),
                "markdown_path": first_issue_string(&page_issues, "markdown_path"),
                "source_path": first_issue_string(&page_issues, "source_path"),
                "route_index_path": first_issue_string(&page_issues, "route_index_path"),
                "broken_internal_count": page_issues.len(),
                "hrefs": issue_strings(&page_issues, "href"),
                "targets": issue_strings(&page_issues, "target"),
                "next_action": "repair_links",
                "suggested_operations": [
                    "wiki.page.open",
                    "wiki.page.patch_body",
                    "wiki.publish",
                    "wiki.validate"
                ]
            })
        })
        .collect()
}

fn link_repair_tasks_from_delete_issues(issues: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut grouped = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for issue in issues {
        if !issue
            .get("code")
            .and_then(|value| value.as_str())
            .is_some_and(|code| code == "would_break_internal_link")
        {
            continue;
        }
        let page_id = issue
            .get("source_page_id")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let route = issue
            .get("source_route")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        grouped
            .entry(format!("{page_id}\0{route}"))
            .or_default()
            .push(issue.clone());
    }

    grouped
        .into_values()
        .map(|page_issues| {
            json!({
                "page_id": first_issue_string(&page_issues, "source_page_id").unwrap_or_else(|| "unknown".to_string()),
                "route": first_issue_string(&page_issues, "source_route").unwrap_or_default(),
                "source_path": first_issue_string(&page_issues, "source_path"),
                "broken_internal_count": page_issues.len(),
                "hrefs": issue_strings(&page_issues, "href"),
                "targets": issue_strings(&page_issues, "target"),
                "target_kinds": issue_strings(&page_issues, "target_kind"),
                "next_action": "repair_links",
                "suggested_operations": [
                    "wiki.page.open",
                    "wiki.page.patch_body",
                    "wiki.publish",
                    "wiki.validate"
                ]
            })
        })
        .collect()
}

fn first_issue_string(issues: &[serde_json::Value], key: &str) -> Option<String> {
    issues.iter().find_map(|issue| {
        issue
            .get(key)
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
    })
}

fn issue_strings(issues: &[serde_json::Value], key: &str) -> Vec<String> {
    issues
        .iter()
        .filter_map(|issue| {
            issue
                .get(key)
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn annotate_link_diagnostics(output: &Path, diagnostics: &serde_json::Value) -> Result<()> {
    let metadata = output.join(".1context");
    fs::create_dir_all(&metadata)?;
    fs::write(
        metadata.join("link-diagnostics.json"),
        serde_json::to_vec_pretty(diagnostics)?,
    )?;

    let Some(issues) = diagnostics.get("issues").and_then(|value| value.as_array()) else {
        return Ok(());
    };
    if issues.is_empty() {
        return Ok(());
    }

    let mut by_source = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for issue in issues {
        if issue
            .get("code")
            .and_then(|value| value.as_str())
            .is_some_and(|code| code == "broken_internal_link")
        {
            for key in ["source_path", "route_index_path"] {
                if let Some(source_path) = issue.get(key).and_then(|value| value.as_str()) {
                    by_source
                        .entry(source_path.to_string())
                        .or_default()
                        .push(issue.clone());
                }
            }
        }
    }

    for (source_path, source_issues) in by_source {
        let html_path = output.join(&source_path);
        if !html_path.is_file() {
            continue;
        }
        let mut html = fs::read_to_string(&html_path)
            .with_context(|| format!("read {}", html_path.display()))?;
        for issue in &source_issues {
            if let Some(href) = issue.get("href").and_then(|value| value.as_str()) {
                html = annotate_broken_anchor(&html, href);
            }
        }
        html = insert_link_warning_banner(&html, &source_issues);
        fs::write(&html_path, html).with_context(|| format!("write {}", html_path.display()))?;
    }

    Ok(())
}

fn annotate_route_manifest_link_diagnostics(
    output: &Path,
    diagnostics: &serde_json::Value,
) -> Result<()> {
    let path = output.join(".1context/route-manifest.json");
    if !path.is_file() {
        return Ok(());
    }
    let mut manifest = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&path).with_context(|| format!("read {}", path.display()))?,
    )?;
    manifest["link_diagnostics"] = json!({
        "path": ".1context/link-diagnostics.json",
        "status": diagnostics.get("status").and_then(|value| value.as_str()).unwrap_or("unknown"),
        "issue_count": diagnostics.get("issue_count").and_then(|value| value.as_u64()).unwrap_or(0),
        "health": link_health_from_diagnostics(diagnostics)
    });
    fs::write(&path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn annotate_broken_anchor(html: &str, href: &str) -> String {
    let escaped_href = escape_html_attr(href);
    let mut hrefs = BTreeSet::new();
    hrefs.insert(escaped_href.as_str());
    hrefs.insert(href);
    let mut out = html.to_string();
    for href in hrefs {
        out = annotate_anchor_href(&out, href);
    }
    out
}

fn annotate_anchor_href(html: &str, href: &str) -> String {
    let mut out = String::with_capacity(html.len() + 128);
    let mut rest = html;
    while let Some(start) = rest.find("<a ") {
        out.push_str(&rest[..start]);
        let anchor = &rest[start..];
        let Some(end) = anchor.find('>') else {
            out.push_str(anchor);
            return out;
        };
        let tag = &anchor[..end];
        if anchor_tag_has_href(tag, href) && !tag.contains("data-1context-link-state=\"broken\"") {
            out.push_str(&annotate_anchor_tag(tag));
        } else {
            out.push_str(tag);
        }
        out.push('>');
        rest = &anchor[end + 1..];
    }
    out.push_str(rest);
    out
}

fn anchor_tag_has_href(tag: &str, href: &str) -> bool {
    tag.contains(&format!("href=\"{href}\"")) || tag.contains(&format!("href='{href}'"))
}

fn annotate_anchor_tag(tag: &str) -> String {
    let mut annotated = if tag.contains("class=\"") {
        tag.replacen("class=\"", "class=\"opctx-broken-link ", 1)
    } else if tag.contains("class='") {
        tag.replacen("class='", "class='opctx-broken-link ", 1)
    } else {
        format!("{tag} class=\"opctx-broken-link\"")
    };
    if !annotated.contains("data-1context-link-state=") {
        annotated.push_str(" data-1context-link-state=\"broken\"");
    }
    if !annotated.contains("title=") {
        annotated.push_str(" title=\"Broken internal link\"");
    }
    if !annotated.contains("aria-label=") {
        annotated.push_str(" aria-label=\"Broken internal link\"");
    }
    annotated
}

fn insert_link_warning_banner(html: &str, issues: &[serde_json::Value]) -> String {
    if html.contains("data-1context-link-diagnostics=\"broken-internal-links\"") {
        return html.to_string();
    }
    let targets = issues
        .iter()
        .filter_map(|issue| issue.get("target").and_then(|value| value.as_str()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let target_list = if targets.is_empty() {
        String::new()
    } else {
        let items = targets
            .iter()
            .map(|target| format!("<li><code>{}</code></li>", escape_html(target)))
            .collect::<Vec<_>>()
            .join("");
        format!("<ul>{items}</ul>")
    };
    let noun = if issues.len() == 1 { "link" } else { "links" };
    let verb = if issues.len() == 1 { "points" } else { "point" };
    let object = if issues.len() == 1 {
        "a missing page"
    } else {
        "missing pages"
    };
    let banner = format!(
        r#"
        <aside class="opctx-link-warning" role="note" data-1context-link-diagnostics="broken-internal-links">
          <strong>Broken internal {noun}</strong>
          <p>{} internal {noun} {verb} to {object}.</p>
          {target_list}
        </aside>"#,
        issues.len()
    );
    if let Some(index) = html.find("<div class=\"opctx-article-body") {
        if let Some(close) = html[index..].find('>') {
            let insert_at = index + close + 1;
            let mut out = String::with_capacity(html.len() + banner.len());
            out.push_str(&html[..insert_at]);
            out.push_str(&banner);
            out.push_str(&html[insert_at..]);
            return out;
        }
    }
    html.replace(
        "<article class=\"opctx-article\">",
        &format!("<article class=\"opctx-article\">{banner}"),
    )
}

fn internal_link_diagnostics(
    output: &Path,
    inventory: Option<&WikiInventory>,
) -> Result<serde_json::Value> {
    let manifest_path = output.join(".1context/route-manifest.json");
    if !manifest_path.is_file() {
        return Ok(json!({
            "status": "warning",
            "issue_count": 1,
            "broken_internal_count": 0,
            "issues": [{
                "code": "route_manifest_missing",
                "severity": "warning",
                "message": "route manifest missing; internal links were not validated"
            }]
        }));
    }

    let manifest = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&manifest_path).with_context(|| format!("read {}", manifest_path.display()))?,
    )?;
    let valid_targets = valid_internal_targets(&manifest);
    let source_lookup = link_source_lookup(&manifest, inventory);
    let manifest_display = display_relative(&manifest_path, output);
    let mut html_files = Vec::new();
    collect_html_files(output, &mut html_files)?;
    html_files.retain(|path| {
        path.file_name().is_none()
            || path.file_name().is_some_and(|name| name != "index.html")
            || path.parent().is_some_and(|parent| parent == output)
    });
    let mut seen = BTreeSet::new();
    let mut issues = Vec::new();

    for html_file in html_files {
        let text = fs::read_to_string(&html_file)
            .with_context(|| format!("read {}", html_file.display()))?;
        let source_path = display_relative(&html_file, output);
        for href in extract_html_attr_values(&text, "href") {
            if let Some(target) = normalize_internal_href(&href, &source_path) {
                if valid_targets.contains(&target)
                    || output_file_target_exists(output, &target)
                    || ignored_internal_href(&target)
                {
                    continue;
                }
                let key = format!("{source_path}\0{href}\0{target}");
                if seen.insert(key) {
                    let source = source_lookup.get(&source_path);
                    issues.push(json!({
                        "code": "broken_internal_link",
                        "severity": "warning",
                        "phase": "post_render_link_check",
                        "source_path": source_path,
                        "page_id": source.and_then(|source| source.page_id.clone()),
                        "route": source.and_then(|source| source.route.clone()),
                        "markdown_path": source.and_then(|source| source.markdown_path.clone()),
                        "route_index_path": source.and_then(|source| source.route_index_path.clone()),
                        "href": href,
                        "target": target,
                        "manifest_path": manifest_display,
                        "suggested_actions": ["edit_source", "replace_link", "publish"],
                        "message": "internal link target is not present in the route manifest"
                    }));
                }
            }
        }
    }

    let broken_internal_count = issues
        .iter()
        .filter(|issue| {
            issue
                .get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "broken_internal_link")
        })
        .count();
    Ok(json!({
        "status": if issues.is_empty() { "ok" } else { "warning" },
        "issue_count": issues.len(),
        "broken_internal_count": broken_internal_count,
        "issues": issues
    }))
}

fn valid_internal_targets(manifest: &serde_json::Value) -> BTreeSet<String> {
    let mut targets = BTreeSet::from(["/".to_string()]);
    if let Some(routes) = manifest.get("routes").and_then(|value| value.as_array()) {
        for route in routes {
            for key in ["route", "html_path", "route_index_path", "markdown_path"] {
                if let Some(value) = route.get(key).and_then(|value| value.as_str()) {
                    add_valid_target(&mut targets, value);
                }
            }
        }
    }
    targets
}

#[derive(Clone, Debug)]
struct LinkSourceInfo {
    page_id: Option<String>,
    route: Option<String>,
    markdown_path: Option<String>,
    route_index_path: Option<String>,
}

fn link_source_lookup(
    manifest: &serde_json::Value,
    inventory: Option<&WikiInventory>,
) -> BTreeMap<String, LinkSourceInfo> {
    let mut lookup = BTreeMap::new();
    if let Some(routes) = manifest.get("routes").and_then(|value| value.as_array()) {
        for route in routes {
            let route_value = route.get("route").and_then(|value| value.as_str());
            let info = LinkSourceInfo {
                page_id: canonical_page_id_for_route(inventory, route_value).or_else(|| {
                    route
                        .get("slug")
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string)
                }),
                route: route_value.map(ToString::to_string),
                markdown_path: route
                    .get("markdown_path")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                route_index_path: route
                    .get("route_index_path")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
            };
            for key in ["html_path", "route_index_path"] {
                if let Some(value) = route.get(key).and_then(|value| value.as_str()) {
                    lookup.insert(value.to_string(), info.clone());
                }
            }
        }
    }
    lookup
}

fn canonical_page_id_for_route(
    inventory: Option<&WikiInventory>,
    route: Option<&str>,
) -> Option<String> {
    let route = route?;
    inventory?
        .pages
        .iter()
        .find(|page| page.route == route)
        .map(|page| page.id.clone())
}

fn add_valid_target(targets: &mut BTreeSet<String>, value: &str) {
    if value.is_empty() {
        return;
    }
    let mut target = if value.starts_with('/') {
        value.to_string()
    } else {
        format!("/{value}")
    };
    target = target.trim_end_matches('/').to_string();
    if target.is_empty() {
        target = "/".to_string();
    }
    targets.insert(target.clone());
    if target != "/" {
        targets.insert(format!("{target}/"));
    }
    if let Some(route) = target.strip_suffix("/index.html") {
        targets.insert(route.to_string());
        targets.insert(format!("{route}/"));
    }
}

fn collect_html_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_html_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "html") {
            files.push(path);
        }
    }
    Ok(())
}

fn extract_html_attr_values(text: &str, attr: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = text;
    let needle = format!("{attr}=");
    while let Some(index) = rest.find(&needle) {
        rest = &rest[index + needle.len()..];
        let Some(quote) = rest.chars().next() else {
            break;
        };
        if quote != '"' && quote != '\'' {
            continue;
        }
        rest = &rest[quote.len_utf8()..];
        if let Some(end) = rest.find(quote) {
            values.push(decode_basic_html_entities(&rest[..end]));
            rest = &rest[end + quote.len_utf8()..];
        } else {
            break;
        }
    }
    values
}

fn decode_basic_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_html_attr(value: &str) -> String {
    escape_html(value)
}

fn normalize_internal_href(href: &str, source_path: &str) -> Option<String> {
    let trimmed = href.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with("tel:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("javascript:")
    {
        return None;
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let normalized = if without_query.starts_with('/') {
        normalize_route_path(without_query)
    } else {
        normalize_relative_route(source_path, without_query)
    }?;
    if normalized == "/" {
        Some("/".to_string())
    } else {
        Some(normalized.trim_end_matches('/').to_string())
    }
}

fn normalize_route_path(path: &str) -> Option<String> {
    normalize_posix_segments(path.trim_start_matches('/'))
}

fn normalize_relative_route(source_path: &str, href: &str) -> Option<String> {
    let base = source_path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("");
    let joined = if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    };
    normalize_posix_segments(&joined)
}

fn normalize_posix_segments(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

fn ignored_internal_href(target: &str) -> bool {
    target.starts_with("/assets/")
        || target.starts_with("/api/")
        || target.starts_with("/.1context/")
        || target == "/favicon.ico"
}

fn output_file_target_exists(output: &Path, target: &str) -> bool {
    let relative = target.trim_start_matches('/');
    if relative.is_empty()
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return false;
    }
    output.join(relative).is_file()
}

fn display_relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn resolve_wiki_engine(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Ok(path) = env::var("ONECONTEXT_WIKI_ENGINE_DIR") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let cwd_candidate = env::current_dir()?.join("wiki-engine");
    if cwd_candidate.join("tools/render-site.mjs").is_file() {
        return Ok(cwd_candidate);
    }
    let exe = env::current_exe()?;
    if let Some(contents) = exe.parent().and_then(|macos| macos.parent()) {
        let bundled = contents.join("Resources/WikiEngine");
        if bundled.join("tools/render-site.mjs").is_file() {
            return Ok(bundled);
        }
    }
    Err(anyhow!(
        "could not locate WikiEngine; pass --wiki-engine or set ONECONTEXT_WIKI_ENGINE_DIR"
    ))
}

fn print_help() {
    println!(
        r#"onecontext-wiki

Usage:
  onecontext-wiki --root <1Context-root> ensure
  onecontext-wiki --root <1Context-root> status
  onecontext-wiki --root <1Context-root> validate
  onecontext-wiki --root <1Context-root> list
  onecontext-wiki --root <1Context-root> page-status <page-id-or-route>
  onecontext-wiki --root <1Context-root> page-open <page-id-or-route>
  onecontext-wiki --root <1Context-root> page-create <page-id> [--title <title>] [--route <route>] [--slug <slug>] [--family-group <id>] [--family-id <id>] [--type <type>] [--template <path>] [--summary <text>] [--nav-section primary|utility|hidden] [--nav-order <n>]
  onecontext-wiki --root <1Context-root> page-create-all
  onecontext-wiki --root <1Context-root> page-write-body <page-id-or-route> (--body <markdown> | --body-file <path>) [--expected-source-sha256 <hash>]
  onecontext-wiki --root <1Context-root> page-patch-body <page-id-or-route> (--find <markdown> | --find-file <path>) (--replace <markdown> | --replace-file <path>) [--expected-source-sha256 <hash>]
  onecontext-wiki --root <1Context-root> asset-add <page-id-or-route> --file <path> [--filename <name>] [--purpose inline_image|download|decorative|source_file] [--caption <text>] [--alt-text <text>]
  onecontext-wiki --root <1Context-root> asset-list <page-id-or-route>
  onecontext-wiki --root <1Context-root> reference-list [page-id-or-route]
  onecontext-wiki --root <1Context-root> page-delete <page-id-or-route> [--mode tombstone]
  onecontext-wiki --root <1Context-root> page-restore <page-id-or-route>
  onecontext-wiki --root <1Context-root> publish-status
  onecontext-wiki --root <1Context-root> publish [--wiki-engine <dir>] [--node <node>] [--trigger <label>] [--force]
  onecontext-wiki --root <1Context-root> agent-list
  onecontext-wiki --root <1Context-root> agent-whoami (--agent-id <agent-id> | --thread-id <codex-thread-id>)
  onecontext-wiki --root <1Context-root> agent-identify --thread-id <codex-thread-id> [--role <address>]... [--capability <name>]... [--ttl-seconds <seconds>]
  onecontext-wiki --root <1Context-root> agent-heartbeat <agent-id> [--ttl-seconds <seconds>]
  onecontext-wiki --root <1Context-root> agent-retire <agent-id>
  onecontext-wiki --root <1Context-root> agent-status <agent-id>
  onecontext-wiki --root <1Context-root> agent-status-by-thread <codex-thread-id>
  onecontext-wiki --root <1Context-root> agent-inbox <agent-id>
  onecontext-wiki --root <1Context-root> mail-send --from <address> --to <address>... --subject <text> (--body <markdown> | --body-file <path>) [--cc <address>]... [--kind <kind>] [--idempotency-key <key>] [--message-id <id>] [--thread-id <id>] [--reply-to <message-id>] [--page-id <id> --page-route <route>] [--body-sha256 <hash>] [--attachment-ref-json <json>]... [--max-open-deliveries-per-recipient <n>]
  onecontext-wiki --root <1Context-root> mail-inbox <address>
  onecontext-wiki --root <1Context-root> mail-read (--message-id <message-id> | --thread-id <thread-id>)
  onecontext-wiki --root <1Context-root> mail-open <delivery-id> --agent-id <agent-id>
  onecontext-wiki --root <1Context-root> mail-record-injection <delivery-id> --agent-id <agent-id> [--thread-id <thread-id>] [--result ok|failed] [--item-count <n>] [--error <text>]
  onecontext-wiki --root <1Context-root> mail-claim <delivery-id> --agent-id <agent-id>
  onecontext-wiki --root <1Context-root> mail-mark <delivery-id> --agent-id <agent-id> --state read|done|archived|rejected
  onecontext-wiki --root <1Context-root> mail-mark-all <agent-id> --state read|done|archived|rejected [--recipient <address>] [--dry-run]
  onecontext-wiki --root <1Context-root> mail-snooze <delivery-id> --agent-id <agent-id> --until <rfc3339>
  onecontext-wiki --root <1Context-root> notify-poll <agent-id> [--cursor <cursor>]
  onecontext-wiki --root <1Context-root> notify-ack <notification-id> --agent-id <agent-id>
  onecontext-wiki --root <1Context-root> notify-dispatch <agent-id> (--dry-run | --steering-command <command> [--steering-arg <arg>]...) [--payload-format text|json] [--limit <n>]
  onecontext-wiki --root <1Context-root> talk-append --page <id> --kind <kind> --subject <subject> --from <actor> (--body <markdown> | --body-file <path>) [--to <label>] [--cc <label>] [--thread-id <thread>] [--reply-to <message>] [--operation-id <id>] [--delivery-mode labels-only|mail] [--attachment <file>] [--attachment-filename <name>] [--attachment-caption <caption>] [--attachment-alt <text>] [--allow-tombstoned]

Aliases: command names may also use the dotted tool form or be prefixed with
wiki-, for example wiki.mail.open, wiki-list, or wiki-page-status. This keeps
the CLI close to the API verb names agents see in receipts and docs.
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn dotted_tool_names_canonicalize_to_cli_commands() {
        assert_eq!(canonical_command("wiki.mail.open"), "mail-open");
        assert_eq!(canonical_command("wiki.notify.ack"), "notify-ack");
        assert_eq!(
            canonical_command("wiki.agent.status_by_thread"),
            "agent-status-by-thread"
        );
        assert_eq!(
            canonical_command("wiki.mail.record_injection"),
            "mail-record-injection"
        );
        assert_eq!(
            operation_for_command("wiki.mail.mark_all"),
            "wiki.mail.mark_all"
        );
        assert_eq!(
            operation_for_command("wiki.agent.status_by_thread"),
            "wiki.agent.status_by_thread"
        );
        assert_eq!(
            operation_for_command("wiki.mail.record_injection"),
            "wiki.mail.record_injection"
        );
    }

    #[test]
    fn cli_sha256_matches_known_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn source_hash_mismatch_error_has_structured_details() {
        let details =
            error_details("source hash mismatch for topics; expected abc123, found def456")
                .unwrap();

        assert_eq!(details["page"], "topics");
        assert_eq!(details["expected_source_sha256"], "abc123");
        assert_eq!(details["found_source_sha256"], "def456");
    }

    #[test]
    fn talk_body_source_errors_have_direct_repair_hints() {
        assert_eq!(
            repair_hints("talk-append requires --body or --body-file"),
            vec![
                "Provide exactly one talk body source: --body <markdown> for short notes, or --body-file <path> for longer notes."
            ]
        );
    }

    #[test]
    fn tombstoned_body_edit_errors_point_to_restore() {
        let message = "page body edit refused for tombstoned page scratch; use wiki.page.restore before editing or choose an enabled page";
        assert_eq!(error_code(message), "tombstoned_page");
        assert_eq!(
            repair_hints(message),
            vec![
                "Run wiki.page.restore before editing, or choose an enabled replacement page for new content."
            ]
        );
    }

    #[test]
    fn link_health_groups_repair_tasks_for_agents() {
        let diagnostics = json!({
            "status": "warning",
            "issue_count": 2,
            "broken_internal_count": 2,
            "issues": [
                {
                    "code": "broken_internal_link",
                    "page_id": "repair-source",
                    "route": "/repair-source",
                    "markdown_path": "repair-source.md",
                    "source_path": "repair-source.html",
                    "route_index_path": "repair-source/index.html",
                    "href": "/deleted-target",
                    "target": "/deleted-target"
                },
                {
                    "code": "broken_internal_link",
                    "page_id": "repair-source",
                    "route": "/repair-source",
                    "markdown_path": "repair-source.md",
                    "source_path": "repair-source.html",
                    "route_index_path": "repair-source/index.html",
                    "href": "/deleted-target.md",
                    "target": "/deleted-target.md"
                }
            ]
        });

        let health = link_health_from_diagnostics(&diagnostics);
        assert_eq!(health["status"], "warning");
        assert_eq!(health["broken_internal_count"], 2);
        assert_eq!(health["pages_with_broken_links"], json!(["repair-source"]));
        assert_eq!(health["repair_tasks"].as_array().unwrap().len(), 1);
        assert_eq!(health["repair_tasks"][0]["page_id"], "repair-source");
        assert_eq!(health["repair_tasks"][0]["broken_internal_count"], 2);
        assert_eq!(
            health["repair_tasks"][0]["suggested_operations"],
            json!([
                "wiki.page.open",
                "wiki.page.patch_body",
                "wiki.publish",
                "wiki.validate"
            ])
        );
    }

    #[test]
    fn delete_lifecycle_turns_link_impact_into_timeline_and_repair_tasks() {
        let mut receipt = json!({
            "schema_version": 1,
            "status": "ok",
            "operation": "wiki.page.delete",
            "id": "deleted-target",
            "render_required": true,
            "next_action": "publish_then_repair_links",
            "link_impact": {
                "status": "warning",
                "deleted_route": "/deleted-target",
                "deleted_markdown_path": "/deleted-target.md",
                "post_publish_expected_next_action": "repair_links",
                "inbound_link_count": 2,
                "source_page_count": 1,
                "issues": [
                    {
                        "code": "would_break_internal_link",
                        "source_page_id": "repair-source",
                        "source_route": "/repair-source",
                        "source_path": "user-wiki/source/repair-source.md",
                        "href": "/deleted-target",
                        "target": "/deleted-target",
                        "target_kind": "route"
                    },
                    {
                        "code": "would_break_internal_link",
                        "source_page_id": "repair-source",
                        "source_route": "/repair-source",
                        "source_path": "user-wiki/source/repair-source.md",
                        "href": "/deleted-target.md",
                        "target": "/deleted-target.md",
                        "target_kind": "markdown"
                    }
                ]
            }
        });

        attach_delete_link_repair_lifecycle(&mut receipt);

        let lifecycle = &receipt["link_repair_lifecycle"];
        assert_eq!(lifecycle["status"], "warning");
        assert_eq!(lifecycle["next_action"], "publish_then_repair_links");
        assert_eq!(lifecycle["branch"]["state"], "publish_then_repair_links");
        assert_eq!(lifecycle["branch"]["terminal"], false);
        assert_eq!(lifecycle["branch"]["next_command"], "wiki.publish");
        assert_eq!(
            lifecycle["branch"]["followup_commands"],
            json!([
                "wiki.page.open",
                "wiki.page.patch_body",
                "wiki.publish",
                "wiki.validate"
            ])
        );
        assert_eq!(lifecycle["timeline"].as_array().unwrap().len(), 2);
        assert_eq!(lifecycle["timeline"][0]["phase"], "pre_delete_preview");
        assert_eq!(
            lifecycle["timeline"][0]["next_action"],
            "publish_then_repair_links"
        );
        assert_eq!(
            lifecycle["timeline"][1]["phase"],
            "post_publish_expected_repair"
        );
        assert_eq!(lifecycle["repair_tasks"].as_array().unwrap().len(), 1);
        assert_eq!(lifecycle["repair_tasks"][0]["page_id"], "repair-source");
        assert_eq!(lifecycle["repair_tasks"][0]["broken_internal_count"], 2);
        assert_eq!(
            lifecycle["repair_tasks"][0]["target_kinds"],
            json!(["markdown", "route"])
        );
    }

    #[test]
    fn publish_lifecycle_matches_post_publish_repair_tasks() {
        let diagnostics = json!({
            "status": "warning",
            "issue_count": 1,
            "broken_internal_count": 1,
            "issues": [
                {
                    "code": "broken_internal_link",
                    "page_id": "repair-source",
                    "route": "/repair-source",
                    "markdown_path": "repair-source.md",
                    "source_path": "repair-source.html",
                    "route_index_path": "repair-source/index.html",
                    "href": "/deleted-target",
                    "target": "/deleted-target"
                }
            ]
        });

        let lifecycle = link_repair_lifecycle_from_diagnostics(
            &diagnostics,
            Some("user-wiki/site/.1context/link-diagnostics.json"),
        );

        assert_eq!(lifecycle["status"], "warning");
        assert_eq!(lifecycle["next_action"], "repair_links");
        assert_eq!(lifecycle["branch"]["state"], "repair_links");
        assert_eq!(lifecycle["branch"]["terminal"], false);
        assert_eq!(lifecycle["branch"]["next_command"], "wiki.page.open");
        assert_eq!(
            lifecycle["branch"]["followup_commands"],
            json!(["wiki.page.patch_body", "wiki.publish", "wiki.validate"])
        );
        assert_eq!(lifecycle["timeline"].as_array().unwrap().len(), 1);
        assert_eq!(lifecycle["timeline"][0]["phase"], "post_publish_link_check");
        assert_eq!(
            lifecycle["timeline"][0]["diagnostics_path"],
            "user-wiki/site/.1context/link-diagnostics.json"
        );
        assert_eq!(lifecycle["repair_tasks"].as_array().unwrap().len(), 1);
        assert_eq!(lifecycle["repair_tasks"][0]["page_id"], "repair-source");
    }

    #[test]
    fn clean_publish_lifecycle_is_terminal_for_agents() {
        let diagnostics = json!({
            "status": "ok",
            "issue_count": 0,
            "broken_internal_count": 0,
            "issues": []
        });

        let lifecycle = link_repair_lifecycle_from_diagnostics(&diagnostics, None);

        assert_eq!(lifecycle["status"], "ok");
        assert_eq!(lifecycle["next_action"], "none");
        assert_eq!(lifecycle["branch"]["state"], "clean");
        assert_eq!(lifecycle["branch"]["terminal"], true);
        assert_eq!(lifecycle["branch"]["next_command"], "none");
        assert_eq!(lifecycle["branch"]["followup_commands"], json!([]));
        assert_eq!(lifecycle["repair_tasks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn publish_preflight_receipt_names_publish_preflight_not_page_create_all() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let wiki_engine = temp.path().join("wiki-engine");
        seed_fake_renderer(&wiki_engine);

        let core = WikiCore::new(&root);
        let receipt = publish(
            &core,
            &root,
            Some(&wiki_engine),
            "sh",
            "receipt-naming-test",
            false,
        )
        .unwrap();

        let preflight = receipt
            .get("preflight")
            .and_then(|value| value.as_array())
            .expect("publish receipt preflight array");
        assert_eq!(preflight.len(), 1);
        assert_eq!(preflight[0]["operation"], "wiki.publish.preflight");
        assert_eq!(preflight[0]["action"], "backfill_configured_pages");
        assert_eq!(
            preflight[0]["result"]["operation"],
            "wiki.publish.preflight"
        );
        assert!(!serde_json::to_string(&preflight)
            .unwrap()
            .contains("wiki.page.create_all"));
    }

    #[test]
    fn failed_publish_preserves_existing_site() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let wiki_engine = temp.path().join("wiki-engine");
        seed_fake_renderer(&wiki_engine);

        let core = WikiCore::new(&root);
        let first = publish(
            &core,
            &root,
            Some(&wiki_engine),
            "sh",
            "initial-publish",
            false,
        )
        .unwrap();
        assert_eq!(first["status"], "published");

        let published_html = root.join("user-wiki/site/topics.html");
        let previous_html = fs::read_to_string(&published_html).unwrap();
        assert!(previous_html.contains("Topics"));

        let source = root.join("user-wiki/source/families/reference/topics/source/topics.md");
        let mut source_text = fs::read_to_string(&source).unwrap();
        source_text.push_str("\n\nDirty source that requires another publish.\n");
        fs::write(&source, source_text).unwrap();

        let failing_engine = temp.path().join("failing-wiki-engine");
        seed_failing_renderer(&failing_engine);
        let failed = publish(
            &core,
            &root,
            Some(&failing_engine),
            "sh",
            "failed-publish",
            false,
        )
        .unwrap();

        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["render_result"]["status"], "failed");
        assert_eq!(fs::read_to_string(&published_html).unwrap(), previous_html);
        assert!(
            !root.join("user-wiki/site/partial.html").exists(),
            "failed staging output must not replace the last-good site"
        );
        assert!(
            root.join("context-engine/runs/wiki-publish-staging/partial.html")
                .exists(),
            "failed staging output should remain available for diagnostics"
        );
    }

    #[test]
    fn publish_uses_validation_repair_hints_for_source_frontmatter_blocks() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let wiki_engine = temp.path().join("wiki-engine");
        seed_fake_renderer(&wiki_engine);

        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let source = root.join("user-wiki/source/families/reference/topics/source/topics.md");
        let text = fs::read_to_string(&source).unwrap().replace(
            "access: \"private\"",
            "access: \"impossible-render-access\"",
        );
        fs::write(&source, text).unwrap();

        let failed = publish(
            &core,
            &root,
            Some(&wiki_engine),
            "sh",
            "blocked-source-frontmatter",
            false,
        )
        .unwrap();

        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["next_action"], "repair_source");
        assert_eq!(failed["render_required"], false);
        assert_eq!(failed["validation"]["can_publish"], false);
        assert!(serde_json::to_string(&failed["validation"]["issues"])
            .unwrap()
            .contains("invalid_page_frontmatter"));
        assert!(serde_json::to_string(&failed["repair_hints"])
            .unwrap()
            .contains("Repair the page source frontmatter"));
    }

    #[test]
    fn agent_facing_commands_reject_stray_arguments_before_mutating() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let root_flag = root.display().to_string();

        for args in [
            vec!["page-status", "topics", "unexpected"],
            vec!["page-open", "topics", "unexpected"],
            vec!["page-create", "scratch", "--title", "Scratch", "unexpected"],
            vec!["page-create-all", "unexpected"],
            vec!["page-write-body", "topics", "--body", "body", "unexpected"],
            vec!["reference-list", "topics", "unexpected"],
            vec![
                "page-patch-body",
                "topics",
                "--find",
                "body",
                "--replace",
                "new body",
                "unexpected",
            ],
            vec!["page-delete", "topics", "unexpected"],
            vec!["page-restore", "topics", "unexpected"],
            vec!["publish-status", "unexpected"],
            vec!["publish", "--force", "unexpected"],
            vec![
                "talk-append",
                "--page",
                "topics",
                "--kind",
                "proposal",
                "--subject",
                "Subject",
                "--from",
                "agent://author",
                "--to",
                "agent://reviewer",
                "--body",
                "Body",
                "unexpected",
            ],
        ] {
            let mut command = vec!["--root".to_string(), root_flag.clone()];
            command.extend(args.into_iter().map(String::from));
            let error = run(command).expect_err("stray argument should fail");
            let message = format!("{error:#}");
            assert!(
                message.contains("received unexpected argument(s): unexpected"),
                "unexpected message for stray arg: {message}"
            );
        }
    }

    #[test]
    fn numeric_cli_flags_reject_bad_or_missing_values_before_mutating() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let root_flag = root.display().to_string();

        for (args, expected) in [
            (
                vec!["page-create", "scratch", "--nav-order", "late"],
                "invalid numeric flag --nav-order for page-create",
            ),
            (
                vec!["page-create", "scratch", "--nav-order"],
                "page-create requires a value for --nav-order",
            ),
        ] {
            let mut command = vec!["--root".to_string(), root_flag.clone()];
            command.extend(args.into_iter().map(String::from));
            let error = run(command).expect_err("bad numeric flag should fail");
            let message = format!("{error:#}");
            assert!(
                message.contains(expected),
                "expected {expected:?} in {message:?}"
            );
        }

        assert!(
            !root.join("user-wiki/source/scratch.md").exists(),
            "invalid nav order must not create the page source"
        );
    }

    #[test]
    fn value_cli_flags_reject_dangling_or_flag_values_before_mutating() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let root_flag = root.display().to_string();

        for (args, expected) in [
            (
                vec!["page-create", "scratch", "--title"],
                "page-create requires a value for --title",
            ),
            (
                vec![
                    "page-create",
                    "scratch-next-flag",
                    "--title",
                    "--route",
                    "/scratch-next-flag",
                ],
                "page-create requires a value for --title",
            ),
            (
                vec!["publish", "--trigger"],
                "publish requires a value for --trigger",
            ),
            (
                vec!["publish", "--trigger", "--force"],
                "publish requires a value for --trigger",
            ),
            (
                vec![
                    "talk-append",
                    "--page",
                    "topics",
                    "--kind",
                    "proposal",
                    "--subject",
                    "Subject",
                    "--from",
                    "agent://author",
                    "--to",
                    "agent://reviewer",
                    "--body",
                ],
                "talk-append requires a value for --body",
            ),
            (
                vec![
                    "talk-append",
                    "--page",
                    "topics",
                    "--kind",
                    "proposal",
                    "--subject",
                    "Subject",
                    "--from",
                    "agent://author",
                    "--to",
                    "agent://reviewer",
                    "--body-file",
                ],
                "talk-append requires a value for --body-file",
            ),
            (
                vec![
                    "talk-append",
                    "--page",
                    "topics",
                    "--kind",
                    "proposal",
                    "--subject",
                    "Subject",
                    "--from",
                    "agent://author",
                    "--to",
                    "agent://reviewer",
                    "--body",
                    "--attachment",
                    "note.txt",
                ],
                "talk-append requires a value for --body",
            ),
            (
                vec![
                    "talk-append",
                    "--page",
                    "topics",
                    "--kind",
                    "proposal",
                    "--subject",
                    "Subject",
                    "--from",
                    "agent://author",
                    "--to",
                    "agent://reviewer",
                    "--body",
                    "Body",
                    "--attachment",
                ],
                "talk-append requires a value for --attachment",
            ),
            (
                vec![
                    "talk-append",
                    "--page",
                    "topics",
                    "--kind",
                    "proposal",
                    "--subject",
                    "Subject",
                    "--from",
                    "agent://author",
                    "--to",
                    "agent://reviewer",
                    "--body",
                    "Body",
                    "--attachment-filename",
                ],
                "talk-append requires a value for --attachment-filename",
            ),
        ] {
            let mut command = vec!["--root".to_string(), root_flag.clone()];
            command.extend(args.into_iter().map(String::from));
            let error = run(command).expect_err("dangling value flag should fail");
            let message = format!("{error:#}");
            assert!(
                message.contains(expected),
                "expected {expected:?} in {message:?}"
            );
        }

        assert!(
            !root.join("user-wiki/source/scratch.md").exists(),
            "dangling --title must not create a page source"
        );
        assert!(
            !root
                .join("context-engine/runs/wiki-publish-result.json")
                .exists(),
            "dangling --trigger must not run the renderer"
        );
        assert!(
            !root
                .join("user-wiki/source/families/reference/topics/talk/topics.talk")
                .exists(),
            "dangling talk flags must not append talk files"
        );
        for args in [vec!["--root"], vec!["status", "--root"]] {
            let command = args.into_iter().map(String::from).collect();
            let error = run(command).expect_err("dangling --root should fail");
            let message = format!("{error:#}");
            assert!(
                message.contains("onecontext-wiki requires a value for --root"),
                "dangling --root should fail before defaulting: {message}"
            );
        }
    }

    #[test]
    fn talk_append_accepts_body_file_and_rejects_ambiguous_body_sources() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let root_flag = root.display().to_string();
        let body_file = temp.path().join("talk-body.md");
        fs::write(
            &body_file,
            "File-backed talk body.\n\nLonger dogfood note stays readable outside argv.\n",
        )
        .unwrap();

        let missing_body_unknown_page = run(vec![
            "--root".to_string(),
            root_flag.clone(),
            "talk-append".to_string(),
            "--page".to_string(),
            "missing-page".to_string(),
            "--kind".to_string(),
            "proposal".to_string(),
            "--subject".to_string(),
            "Missing Body Before Lookup".to_string(),
            "--from".to_string(),
            "agent://worker-be".to_string(),
            "--to".to_string(),
            "role://topics.curator".to_string(),
        ])
        .expect_err("talk append should validate missing body before page lookup");
        let message = format!("{missing_body_unknown_page:#}");
        assert!(
            message.contains("talk-append requires --body or --body-file"),
            "unexpected required-body error: {message}"
        );
        assert!(
            !message.contains("unknown page"),
            "missing body should not be masked by page lookup: {message}"
        );

        let ambiguous_body_unknown_page = run(vec![
            "--root".to_string(),
            root_flag.clone(),
            "talk-append".to_string(),
            "--page".to_string(),
            "missing-page".to_string(),
            "--kind".to_string(),
            "proposal".to_string(),
            "--subject".to_string(),
            "Ambiguous Body Before Lookup".to_string(),
            "--from".to_string(),
            "agent://worker-be".to_string(),
            "--to".to_string(),
            "role://topics.curator".to_string(),
            "--body".to_string(),
            "inline".to_string(),
            "--body-file".to_string(),
            body_file.display().to_string(),
        ])
        .expect_err("talk append should validate ambiguous body before page lookup");
        let message = format!("{ambiguous_body_unknown_page:#}");
        assert!(
            message.contains("talk-append accepts either --body or --body-file, not both"),
            "unexpected ambiguous-body error: {message}"
        );
        assert!(
            !message.contains("unknown page"),
            "ambiguous body should not be masked by page lookup: {message}"
        );

        run(vec![
            "--root".to_string(),
            root_flag.clone(),
            "page-create-all".to_string(),
        ])
        .unwrap();

        let missing_body = run(vec![
            "--root".to_string(),
            root_flag.clone(),
            "talk-append".to_string(),
            "--page".to_string(),
            "topics".to_string(),
            "--kind".to_string(),
            "proposal".to_string(),
            "--subject".to_string(),
            "Missing Body".to_string(),
            "--from".to_string(),
            "agent://worker-be".to_string(),
            "--to".to_string(),
            "role://topics.curator".to_string(),
        ])
        .expect_err("talk append should require a body source");
        let message = format!("{missing_body:#}");
        assert!(
            message.contains("talk-append requires --body or --body-file"),
            "unexpected required-body error: {message}"
        );

        run(vec![
            "--root".to_string(),
            root_flag.clone(),
            "talk-append".to_string(),
            "--page".to_string(),
            "topics".to_string(),
            "--kind".to_string(),
            "proposal".to_string(),
            "--subject".to_string(),
            "Body File Proof".to_string(),
            "--from".to_string(),
            "agent://worker-be".to_string(),
            "--to".to_string(),
            "role://topics.curator".to_string(),
            "--body-file".to_string(),
            body_file.display().to_string(),
        ])
        .unwrap();

        let talk_dir = root.join("user-wiki/source/families/reference/topics/talk/topics.talk");
        let messages = fs::read_dir(&talk_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "md")
            })
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('_'))
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        let talk_text = fs::read_to_string(messages[0].path()).unwrap();
        assert!(talk_text.contains("File-backed talk body."));
        assert!(talk_text.contains("Longer dogfood note stays readable outside argv."));

        let conflict = run(vec![
            "--root".to_string(),
            root_flag,
            "talk-append".to_string(),
            "--page".to_string(),
            "topics".to_string(),
            "--kind".to_string(),
            "proposal".to_string(),
            "--subject".to_string(),
            "Ambiguous Body".to_string(),
            "--from".to_string(),
            "agent://worker-be".to_string(),
            "--to".to_string(),
            "role://topics.curator".to_string(),
            "--body".to_string(),
            "inline".to_string(),
            "--body-file".to_string(),
            body_file.display().to_string(),
        ])
        .expect_err("talk append should reject both body sources");
        let message = format!("{conflict:#}");
        assert!(
            message.contains("talk-append accepts either --body or --body-file, not both"),
            "unexpected conflict error: {message}"
        );
        assert_eq!(
            fs::read_dir(&talk_dir)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "md"))
                .filter(|entry| !entry.file_name().to_string_lossy().starts_with('_'))
                .count(),
            1,
            "ambiguous body sources must not append a second talk file"
        );
    }

    #[test]
    fn talk_attachment_metadata_flags_align_with_attachments() {
        let attachments = talk_attachment_inputs(
            vec!["/tmp/raw.txt".to_string(), "/tmp/second.txt".to_string()],
            vec!["agent-facing.txt".to_string()],
            vec!["Agent-facing caption".to_string()],
            vec!["Agent-facing alt".to_string()],
        )
        .unwrap();

        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].path, "/tmp/raw.txt");
        assert_eq!(attachments[0].filename.as_deref(), Some("agent-facing.txt"));
        assert_eq!(
            attachments[0].caption.as_deref(),
            Some("Agent-facing caption")
        );
        assert_eq!(attachments[0].alt_text.as_deref(), Some("Agent-facing alt"));
        assert_eq!(attachments[1].path, "/tmp/second.txt");
        assert!(attachments[1].filename.is_none());

        let error = talk_attachment_inputs(vec![], vec!["orphan.txt".to_string()], vec![], vec![])
            .unwrap_err()
            .to_string();
        assert!(error.contains("metadata without a matching --attachment"));
    }

    #[test]
    fn page_create_accepts_integer_nav_order() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);

        run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "page-create".to_string(),
            "scratch".to_string(),
            "--title".to_string(),
            "Scratch".to_string(),
            "--template".to_string(),
            "pages/e08/topics.md".to_string(),
            "--talk-conventions-template".to_string(),
            "talk/conventions/topics.md".to_string(),
            "--talk-curator-template".to_string(),
            "talk/curators/topics.md".to_string(),
            "--nav-order".to_string(),
            "7".to_string(),
        ])
        .unwrap();

        let wiki_toml = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        assert!(
            wiki_toml.contains("nav_order = 7"),
            "integer nav order should be persisted in wiki.toml"
        );
    }

    #[test]
    fn page_create_rejects_invalid_nav_section_before_mutating() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);

        let error = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "page-create".to_string(),
            "bad-nav-section".to_string(),
            "--title".to_string(),
            "Bad Nav Section".to_string(),
            "--template".to_string(),
            "pages/e08/topics.md".to_string(),
            "--talk-conventions-template".to_string(),
            "talk/conventions/topics.md".to_string(),
            "--talk-curator-template".to_string(),
            "talk/curators/topics.md".to_string(),
            "--nav-section".to_string(),
            "banana".to_string(),
        ])
        .expect_err("invalid nav section should be rejected");
        let message = format!("{error:#}");
        assert!(
            message.contains("invalid nav section banana"),
            "unexpected error: {message}"
        );

        let wiki_toml = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        assert!(!wiki_toml.contains("bad-nav-section"));
        assert!(!root
            .join("user-wiki/source/families/custom/bad-nav-section")
            .exists());
    }

    #[test]
    fn concurrent_page_create_commands_preserve_both_registry_entries() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_runtime(&root);
        let root_flag = root.display().to_string();

        let create_args = |page: &str| {
            vec![
                "--root".to_string(),
                root_flag.clone(),
                "page-create".to_string(),
                page.to_string(),
                "--title".to_string(),
                page.to_string(),
                "--template".to_string(),
                "pages/e08/topics.md".to_string(),
                "--talk-conventions-template".to_string(),
                "talk/conventions/topics.md".to_string(),
                "--talk-curator-template".to_string(),
                "talk/curators/topics.md".to_string(),
            ]
        };

        let left = thread::spawn({
            let args = create_args("parallel-alpha");
            move || run(args)
        });
        let right = thread::spawn({
            let args = create_args("parallel-beta");
            move || run(args)
        });

        left.join().unwrap().unwrap();
        right.join().unwrap().unwrap();

        let wiki_toml = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        assert!(wiki_toml.contains("id = \"parallel-alpha\""));
        assert!(wiki_toml.contains("id = \"parallel-beta\""));
        assert!(root
            .join("user-wiki/source/families/custom/parallel-alpha/source/parallel-alpha.md")
            .exists());
        assert!(root
            .join("user-wiki/source/families/custom/parallel-beta/source/parallel-beta.md")
            .exists());
    }

    fn seed_runtime(root: &Path) {
        fs::create_dir_all(root.join("user-wiki/templates/pages/e08")).unwrap();
        fs::create_dir_all(root.join("user-wiki/templates/talk/conventions")).unwrap();
        fs::create_dir_all(root.join("user-wiki/templates/talk/curators")).unwrap();
        fs::write(
            root.join("user-wiki/wiki.toml"),
            r#"
schema_version = 1

[defaults]
template_pack = "e08"

[[pages]]
id = "topics"
enabled = true
title = "Topics"
slug = "topics"
route = "/topics"
family_group = "reference"
family_group_title = "Reference"
family_id = "topics"
family_title = "Topics"
type = "topic-index"
template = "pages/e08/topics.md"
talk_conventions_template = "talk/conventions/topics.md"
talk_curator_template = "talk/curators/topics.md"
"#,
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/pages/e08/topics.md"),
            "---\npage_id: \"{{ page_id }}\"\ntitle: \"{{ title }}\"\nslug: \"{{ slug }}\"\nroute: \"{{ route }}\"\nsection: \"{{ section }}\"\naccess: \"{{ access_tier }}\"\n---\n\n# {{ title }}\n",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/talk/conventions/topics.md"),
            "# {{ title }} Talk Conventions\n",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/talk/curators/topics.md"),
            "# {{ title }} Curator\n",
        )
        .unwrap();
    }

    fn seed_fake_renderer(wiki_engine: &Path) {
        let tools = wiki_engine.join("tools");
        fs::create_dir_all(&tools).unwrap();
        fs::write(
            tools.join("render-site.mjs"),
            r#"#!/bin/sh
output=""
result_json=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      output="$2"
      shift 2
      ;;
    --result-json)
      result_json="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
mkdir -p "$output/.1context" "$output/topics"
cat > "$output/topics.html" <<'HTML'
<main>Topics</main>
HTML
cat > "$output/topics/index.html" <<'HTML'
<main>Topics route index</main>
HTML
cat > "$output/topics.md" <<'MD'
# Topics
MD
cat > "$output/.1context/route-manifest.json" <<'JSON'
{
  "routes": [
    {
      "route": "/topics",
      "html_path": "topics.html",
      "route_index_path": "topics/index.html",
      "markdown_path": "topics.md"
    }
  ]
}
JSON
cat > "$result_json" <<'JSON'
{
  "status": "published",
  "rendered_at": "2026-05-19T00:00:00Z",
  "route_count": 1,
  "markdown_count": 1
}
JSON
"#,
        )
        .unwrap();
    }

    fn seed_failing_renderer(wiki_engine: &Path) {
        let tools = wiki_engine.join("tools");
        fs::create_dir_all(&tools).unwrap();
        fs::write(
            tools.join("render-site.mjs"),
            r#"#!/bin/sh
output=""
result_json=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      output="$2"
      shift 2
      ;;
    --result-json)
      result_json="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
rm -rf "$output"
mkdir -p "$output/.1context"
cat > "$output/partial.html" <<'HTML'
<main>partial failed render</main>
HTML
cat > "$result_json" <<'JSON'
{
  "status": "failed",
  "error": "deliberate renderer failure"
}
JSON
exit 1
"#,
        )
        .unwrap();
    }
}

use std::{collections::HashMap, error::Error, path::PathBuf, str::FromStr};

use anyhow::anyhow;
use cargo_scan::{
    audit_chain::Create,
    audit_file::{AuditFile, EffectInfo},
    auditing::chain::{Command, CommandRunner, OuterArgs},
    effect::{self},
    ident::CanonicalPath,
    scanner::{self},
    util::load_cargo_toml,
};
use home::home_dir;
use log::{debug, info};
use lsp_server::{Connection, Message};
use lsp_types::{
    notification::Notification, request::Request, InitializeParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};
use serde::{Deserialize, Serialize};

use crate::{
    location::to_src_loc,
    notification::{AuditNotification, AuditNotificationParams},
    request::{
        audit_req, scan_req, AuditCommandResponse, CallerCheckedResponse,
        EffectsResponse, ScanCommandResponse,
    },
    util::{
        add_callers_to_tree, find_effect_instance, get_all_chain_effects,
        get_new_audit_locs,
    },
};

#[derive(Serialize, Deserialize, Debug)]
struct ScanCommandParams {
    params: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CallerCheckedCommandParams {
    effect: EffectsResponse,
}

struct ScanCommand;
struct AuditCommand;
struct CallerCheckedCommand;

impl Request for ScanCommand {
    type Params = ScanCommandParams;
    type Result = ScanCommandResponse;
    const METHOD: &'static str = "cargo-scan.scan";
}

impl Request for AuditCommand {
    type Params = ScanCommandParams;
    type Result = ScanCommandResponse;
    const METHOD: &'static str = "cargo-scan.audit";
}

impl Request for CallerCheckedCommand {
    type Params = CallerCheckedCommandParams;
    type Result = String;
    const METHOD: &'static str = "cargo-scan.get_callers";
}

#[derive(Debug, Serialize, Deserialize)]
struct InfoMessageParams {
    pub message: String,
}

impl Notification for InfoMessageParams {
    const METHOD: &'static str = "cargo-scan.info";
    type Params = InfoMessageParams;
}

pub fn run_server() -> anyhow::Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    // Server capabilities
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::FULL,
        )),
        ..ServerCapabilities::default()
    };

    // Read the initialize message from the client
    let initialize_params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;
    info!("Initialized server connection");

    runner(&connection, initialize_params)?;
    io_threads.join()?;

    Ok(())
}

fn runner(
    conn: &lsp_server::Connection,
    init_params: lsp_types::InitializeParams,
) -> anyhow::Result<()> {
    // Extract workspace folders from initialize params
    // and find root path of Cargo Scan's input package
    let workspace_folders = init_params.workspace_folders;

    // Determine the root URI from the first workspace folder if available
    let root_uri = workspace_folders
        .and_then(|folders| folders.first().map(|folder| folder.uri.clone()))
        .ok_or_else(|| anyhow!("Couldn't get root path from workspace folders"))?;

    let root_crate_path = std::path::PathBuf::from_str(root_uri.path())?;
    info!("Crate path received in cargo-scan LSP server: {}", root_crate_path.display());

    let scan_res =
        scanner::scan_crate(&root_crate_path, effect::DEFAULT_EFFECT_TYPES, false, true)?;

    info!("Starting main server loop\n");
    let mut audit_file: Option<AuditFile> = None;
    let mut audit_file_path = PathBuf::new();
    let mut chain_manifest = PathBuf::new();
    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    return Ok(());
                }

                match req.method.as_str() {
                    ScanCommand::METHOD => {
                        let res = scan_req(&root_crate_path)?;
                        conn.sender.send(Message::Response(lsp_server::Response {
                            id: req.id,
                            result: Some(res),
                            error: None,
                        }))?;
                    }
                    AuditCommand::METHOD => {
                        let (af, fp) = audit_req(&root_crate_path)?;
                        let mut effects = HashMap::new();

                        af.clone().audit_trees.iter().for_each(|(eff, tree)| {
                            effects.insert(eff.clone(), tree.get_all_annotations());
                        });

                        audit_file = Some(af);
                        audit_file_path = fp;
                        let res = AuditCommandResponse::new(&effects)?.to_json_value()?;
                        conn.sender.send(Message::Response(lsp_server::Response {
                            id: req.id,
                            result: Some(res),
                            error: None,
                        }))?;
                    }
                    CallerCheckedCommand::METHOD => {
                        let effect = EffectsResponse::from_json_value(req.params)?;
                        let caller_path = CanonicalPath::new_owned(effect.get_caller());
                        let callee_loc = to_src_loc(&effect.location)?;

                        let new_audit_locs = get_new_audit_locs(&scan_res, &caller_path)?;
                        let callers =
                            CallerCheckedResponse::new(&effect, &new_audit_locs)?;

                        if let Some(af) = audit_file.as_mut() {
                            for tree in find_effect_instance(af, effect)? {
                                let curr_effect = EffectInfo {
                                    caller_path: caller_path.clone(),
                                    callee_loc: callee_loc.clone(),
                                };
                                add_callers_to_tree(
                                    new_audit_locs.clone(),
                                    tree,
                                    curr_effect,
                                );
                            }
                            af.recalc_pub_caller_checked(&scan_res.pub_fns);
                            af.save_to_file(audit_file_path.clone())?;
                        }

                        // send the new audit locations to the client
                        let res = callers.to_json_value()?;
                        conn.sender.send(Message::Response(lsp_server::Response {
                            id: req.id,
                            result: Some(res),
                            error: None,
                        }))?;
                    }
                    "cargo-scan.create_chain" => {
                        let outer_args = OuterArgs::default();
                        let root_crate_id = load_cargo_toml(&root_crate_path)?;

                        if let Some(mut dir) = home_dir() {
                            dir.push(".cargo_audits");
                            dir.push("chain_policies");
                            dir.push(format!("{}.manifest", root_crate_id));

                            chain_manifest = dir;
                        };

                        let create_args = Create {
                            crate_path: root_crate_path.to_string_lossy().to_string(),
                            manifest_path: chain_manifest.to_string_lossy().to_string(),
                            ..Default::default()
                        };

                        if !create_args.force_overwrite {
                            let params = InfoMessageParams {
                                message: "May reuse existing audits for chain"
                                    .to_string(),
                            };
                            let value = serde_json::to_value(params.message)?;
                            let notification =
                                Message::Notification(lsp_server::Notification {
                                    method: InfoMessageParams::METHOD.to_string(),
                                    params: value,
                                });
                            conn.sender.send(notification)?;
                        }

                        CommandRunner::run_command(
                            Command::Create(create_args),
                            outer_args,
                        )?;
                    }
                    "cargo-scan.audit_chain" => {
                        let root_crate_id = load_cargo_toml(&root_crate_path)?;
                        chain_manifest = home_dir()
                            .ok_or_else(|| anyhow!("Could not find homw directory"))?;
                        chain_manifest.push(".cargo_audits");
                        chain_manifest.push("chain_policies");
                        chain_manifest.push(format!("{}.manifest", root_crate_id));

                        debug!(
                            "Auditing chain with manifest path: {}",
                            chain_manifest.display()
                        );
                        let effects = get_all_chain_effects(&chain_manifest)?;
                        let res = AuditCommandResponse::new(&effects)?.to_json_value()?;

                        conn.sender.send(Message::Response(lsp_server::Response {
                            id: req.id,
                            result: Some(res),
                            error: None,
                        }))?;
                    }
                    _ => {}
                };
            }
            Message::Response(_) => {}
            Message::Notification(notif) => {
                if notif.method == AuditNotification::METHOD {
                    let params: AuditNotificationParams =
                        serde_json::from_value(notif.params)?;

                    if params.chain_audit_mode {
                        AuditNotification::annotate_effects_in_chain_audit(
                            params,
                            &chain_manifest,
                        )?;
                    } else if let Some(af) = audit_file.as_mut() {
                        AuditNotification::annotate_effects_in_single_audit(
                            params,
                            af,
                            &scan_res,
                            audit_file_path.clone(),
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}

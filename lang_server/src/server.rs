use std::{collections::HashMap, error::Error, path::PathBuf, str::FromStr};

use anyhow::anyhow;
use cargo_scan::{
    audit_chain::Create,
    audit_file::{AuditFile, EffectInfo},
    auditing::chain::{Command, CommandRunner, OuterArgs},
    effect::{self},
    ident::CanonicalPath,
    scanner,
};
use log::{debug, info};
use lsp_server::{Connection, Message};
use lsp_types::{
    notification::Notification, request::Request, InitializeParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};
use serde::{Deserialize, Serialize};

use crate::{
    location::{convert_annotation, to_src_loc},
    notification::{AuditNotification, AuditNotificationParams},
    request::{
        audit_req, scan_req, AuditCommandResponse, CallerCheckedResponse,
        EffectsResponse, ScanCommandResponse,
    },
    util::{add_callers_to_tree, find_effect_instance, get_new_audit_locs},
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
    debug!("Crate path received in cargo-scan LSP server: {}", root_crate_path.display());

    let scan_res =
        scanner::scan_crate(&root_crate_path, effect::DEFAULT_EFFECT_TYPES, false)?;

    info!("Starting main server loop\n");
    let mut audit_file: Option<AuditFile> = None;
    let mut audit_file_path = PathBuf::new();
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
                            if let Some(tree) = find_effect_instance(af, effect)? {
                                let curr_effect = EffectInfo { caller_path, callee_loc };
                                add_callers_to_tree(new_audit_locs, tree, curr_effect);
                                af.save_to_file(audit_file_path.clone())?;
                            }
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
                        let create_args = Create {
                            crate_path: root_crate_path
                                .to_str()
                                .unwrap_or("./")
                                .to_string(),
                            manifest_path: root_crate_path
                                .join("policy.manifest")
                                .to_str()
                                .unwrap_or("./policy.manifest")
                                .to_string(),
                            force_overwrite: true,
                            ..Default::default()
                        };

                        CommandRunner::run_command(
                            Command::Create(create_args),
                            outer_args,
                        )?;
                    }
                    _ => {}
                };
            }
            Message::Response(_) => {}
            Message::Notification(notif) => {
                if notif.method == AuditNotification::METHOD {
                    let params: AuditNotificationParams =
                        serde_json::from_value(notif.params)?;
                    let annotation = params.safety_annotation;
                    let effect = params.effect;

                    if let Some(af) = audit_file.as_mut() {
                        if let Some(tree) = find_effect_instance(af, effect)? {
                            let new_ann = convert_annotation(annotation);
                            tree.set_annotation(new_ann);
                            af.save_to_file(audit_file_path.clone())?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

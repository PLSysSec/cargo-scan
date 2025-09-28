use std::{collections::HashMap, error::Error, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Context};
use cargo_scan::{
    audit_chain::Create,
    audit_file::AuditFile,
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
    notification::{AuditNotification, AuditNotificationParams},
    request::{
        audit_req, scan_req, AuditCommandResponse, EffectsResponse, ScanCommandResponse,
    },
    util::{get_all_chain_effects, get_callers},
};

#[derive(Serialize, Deserialize, Debug)]
struct ScanCommandParams {
    params: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CallerCheckedCommandParams {
    effect: EffectsResponse,
    chain_audit_mode: bool,
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
                        let params: CallerCheckedCommandParams =
                            serde_json::from_value(req.params)?;
                        let effect = params.effect;

                        let callers = if params.chain_audit_mode {
                            let crate_name =
                                CanonicalPath::new_owned(effect.get_caller())
                                    .crate_name()
                                    .to_string();

                            let mut chain =
                                cargo_scan::audit_chain::AuditChain::read_audit_chain(
                                    chain_manifest.to_path_buf(),
                                )?
                                .expect("No audit chain found");
                            let crate_id =
                                chain.resolve_crate_id(&crate_name).context(format!(
                                    "Couldn't resolve crate_name for {}",
                                    &crate_name
                                ))?;

                            let mut af = chain
                                .read_audit_file(&crate_id)?
                                .expect("No audit file for crate");
                            let callers = get_callers(&mut af, effect, &scan_res)?;
                            chain.save_audit_file(&crate_id, &af)?;
                            
                            callers
                        } else {
                            let af = audit_file.as_mut().expect("No audit file found");
                            let callers = get_callers(af, effect, &scan_res)?;
                            af.save_to_file(audit_file_path.clone())?;
                            
                            callers
                        };

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

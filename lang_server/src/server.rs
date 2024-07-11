use std::{error::Error, path::PathBuf, str::FromStr};

use anyhow::anyhow;
use cargo_scan::{audit_file::AuditFile, effect::EffectInstance, ident::CanonicalPath};
use log::{debug, info};
use lsp_server::{Connection, Message};
use lsp_types::{
    notification::Notification, request::Request, InitializeParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};
use serde::{Deserialize, Serialize};

use crate::{
    location::{convert_annotation, to_src_loc},
    request::{
        audit_req, scan_req, AuditCommandResponse, EffectsResponse, ScanCommandResponse,
    },
};

#[derive(Serialize, Deserialize, Debug)]
struct ScanCommandParams {
    params: String,
}

struct ScanCommand;
struct AuditCommand;

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

#[derive(Debug, Deserialize, Serialize)]
struct AuditNotificationParams {
    safety_annotation: String,
    effect: EffectsResponse,
}

struct AuditNotification;

impl Notification for AuditNotification {
    type Params = AuditNotificationParams;
    const METHOD: &'static str = "cargo-scan.set_annotation";
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

    info!("Starting main server loop\n");
    let mut audit_file: Option<AuditFile> = None;
    let mut audit_file_path = PathBuf::new();
    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    return Ok(());
                }

                if req.method == ScanCommand::METHOD {
                    let res = scan_req(&root_crate_path)?;
                    conn.sender.send(Message::Response(lsp_server::Response {
                        id: req.id,
                        result: Some(res),
                        error: None,
                    }))?;
                } else if req.method == AuditCommand::METHOD {
                    let (af, fp) = audit_req(&root_crate_path)?;
                    let effects = af
                        .clone()
                        .audit_trees
                        .into_iter()
                        .map(|(eff, tree)| {
                            let ann = tree
                                .get_leaf_annotation()
                                .map_or_else(String::new, |a| a.to_string());

                            (eff, ann)
                        })
                        .collect::<Vec<(EffectInstance, String)>>();

                    audit_file = Some(af);
                    audit_file_path = fp;
                    let res = AuditCommandResponse::new(&effects)?.to_json_value()?;
                    conn.sender.send(Message::Response(lsp_server::Response {
                        id: req.id,
                        result: Some(res),
                        error: None,
                    }))?;
                }
            }
            Message::Response(_) => {}
            Message::Notification(notif) => {
                if notif.method == AuditNotification::METHOD {
                    let params: AuditNotificationParams =
                        serde_json::from_value(notif.params)?;
                    let annotation = params.safety_annotation;
                    let effect = params.effect;
                    let src_loc = to_src_loc(effect.location)?;
                    let callee = CanonicalPath::new_owned(effect.callee);

                    if let Some(audit_) = audit_file.as_mut() {
                        if let Some((_, tree)) =
                            audit_.audit_trees.iter_mut().find(|(i, _)| {
                                *i.callee() == callee && *i.call_loc() == src_loc
                            })
                        {
                            let new_ann = convert_annotation(annotation);
                            tree.set_annotation(new_ann);
                            audit_.save_to_file(audit_file_path.clone())?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

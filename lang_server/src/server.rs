use std::{error::Error, str::FromStr};

use anyhow::anyhow;
use log::{debug, info};
use lsp_server::{Connection, Message};
use lsp_types::{
    request::Request, InitializeParams, Location, Position, Range, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use serde::{Deserialize, Serialize};

use cargo_scan::scan_stats;

#[derive(Serialize, Deserialize, Debug)]
struct ScanCommandParams {
    params: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct ScanCommandResponse {
    effects: Vec<EffectsResponse>,
}

#[derive(Serialize, Deserialize, Debug)]
struct EffectsResponse {
    effect_id: String,
    effect_type: String,
    location: Location,
}

struct ScanCommand;

impl Request for ScanCommand {
    type Params = ScanCommandParams;
    type Result = ScanCommandResponse;
    const METHOD: &'static str = "cargo-scan.scan";
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
        .and_then(|folders| folders.get(0).map(|folder| folder.uri.clone()))
        .ok_or_else(|| anyhow!("Couldn't get root path from workspace folders"))?;

    let root_crate_path = root_uri.path();
    info!("Starting main server loop");

    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    return Ok(());
                }

                if req.method == ScanCommand::METHOD {
                    // scan crate in root path
                    let path = std::path::PathBuf::from_str(root_crate_path)?;
                    debug!(
                        "Crate path received in cargo-scan lsp server: {}",
                        path.display()
                    );

                    let res = scan_stats::get_crate_stats_default(path, false);
                    info!("Finished scanning. Found {} effects.", res.effects.len());

                    // Gather effects to send as a response
                    let mut effects = vec![];
                    for effect in res.effects {
                        let call_loc = effect.call_loc();
                        let filepath = call_loc.filepath_string();

                        // Convert call location to an appropriate lsp type
                        let location = Location {
                            uri: Url::from_file_path(filepath).unwrap(),
                            range: Range {
                                start: Position {
                                    line: call_loc.sub1().start_line() as u32,
                                    character: call_loc.sub1().start_col() as u32,
                                },
                                end: Position {
                                    line: call_loc.sub1().end_line() as u32,
                                    character: call_loc.sub1().end_col() as u32,
                                },
                            },
                        };

                        let eff = EffectsResponse {
                            effect_id: effect.callee_path().to_string(),
                            effect_type: effect.eff_type().to_csv(),
                            location,
                        };

                        effects.push(eff);
                    }

                    let res = serde_json::to_value(ScanCommandResponse { effects })?;
                    conn.sender.send(Message::Response(lsp_server::Response {
                        id: req.id,
                        result: Some(res),
                        error: None,
                    }))?;
                }
            }
            Message::Response(_) => {}
            Message::Notification(_) => {}
        }
    }

    Ok(())
}

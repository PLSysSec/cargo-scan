use lsp_types::notification::Notification;
use serde::{Deserialize, Serialize};

use crate::request::EffectsResponse;

#[derive(Debug, Deserialize, Serialize)]
pub struct AuditNotificationParams {
    pub safety_annotation: String,
    pub effect: EffectsResponse,
}

pub struct AuditNotification;

impl Notification for AuditNotification {
    type Params = AuditNotificationParams;
    const METHOD: &'static str = "cargo-scan.set_annotation";
}

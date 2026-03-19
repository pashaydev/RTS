use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const PUBLIC_WS_PATH_PREFIX: &str = "/session";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedSessionRegistration {
    pub code: String,
    pub app: Option<String>,
    pub machine_id: String,
    pub region: Option<String>,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedSessionRecord {
    pub code: String,
    pub app: Option<String>,
    pub machine_id: String,
    pub region: Option<String>,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedSessionSummary {
    pub code: String,
    pub ws_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlyReplayInstruction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    pub instance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<FlyReplayTransform>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlyReplayTransform {
    pub path: String,
}

#[derive(Debug, Clone, Default)]
pub struct SessionDirectory {
    sessions: HashMap<String, HostedSessionRecord>,
}

impl SessionDirectory {
    pub fn register(
        &mut self,
        registration: HostedSessionRegistration,
    ) -> Result<HostedSessionRecord, String> {
        let code = registration.code.trim();
        if !is_valid_hosted_session_code(code) {
            return Err(
                "Hosted session codes must be 4-32 characters using letters, numbers, - or _."
                    .to_string(),
            );
        }

        let machine_id = registration.machine_id.trim();
        if machine_id.is_empty() {
            return Err("Hosted session machine_id is required".to_string());
        }

        let target_path = normalize_target_path(&registration.target_path)?;
        let record = HostedSessionRecord {
            code: code.to_string(),
            app: registration.app.filter(|value| !value.trim().is_empty()),
            machine_id: machine_id.to_string(),
            region: registration.region.filter(|value| !value.trim().is_empty()),
            target_path,
        };
        self.sessions.insert(record.code.clone(), record.clone());
        Ok(record)
    }

    pub fn get(&self, code: &str) -> Option<HostedSessionRecord> {
        self.sessions.get(code.trim()).cloned()
    }
}

pub type SharedSessionDirectory = Arc<RwLock<SessionDirectory>>;

pub fn make_shared_directory() -> SharedSessionDirectory {
    Arc::new(RwLock::new(SessionDirectory::default()))
}

pub fn is_valid_hosted_session_code(code: &str) -> bool {
    let trimmed = code.trim();
    let len_ok = (4..=32).contains(&trimmed.len());
    len_ok
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn public_ws_path(code: &str) -> Result<String, String> {
    let trimmed = code.trim();
    if !is_valid_hosted_session_code(trimmed) {
        return Err(
            "Hosted session codes must be 4-32 characters using letters, numbers, - or _."
                .to_string(),
        );
    }
    Ok(format!("{}/{}/ws", PUBLIC_WS_PATH_PREFIX, trimmed))
}

pub fn replay_instruction(record: &HostedSessionRecord) -> FlyReplayInstruction {
    FlyReplayInstruction {
        app: record.app.clone(),
        instance: record.machine_id.clone(),
        region: record.region.clone(),
        transform: Some(FlyReplayTransform {
            path: record.target_path.clone(),
        }),
    }
}

fn normalize_target_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Hosted session target_path is required".to_string());
    }
    if !trimmed.starts_with('/') {
        return Err("Hosted session target_path must start with '/'".to_string());
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_lookup_session() {
        let mut directory = SessionDirectory::default();
        let record = directory
            .register(HostedSessionRegistration {
                code: "ABCD12".to_string(),
                app: Some("rts-game".to_string()),
                machine_id: "3d8e19f".to_string(),
                region: Some("ams".to_string()),
                target_path: "/ws".to_string(),
            })
            .unwrap();

        assert_eq!(
            directory.get("ABCD12"),
            Some(HostedSessionRecord {
                code: "ABCD12".to_string(),
                app: Some("rts-game".to_string()),
                machine_id: "3d8e19f".to_string(),
                region: Some("ams".to_string()),
                target_path: "/ws".to_string(),
            })
        );
        assert_eq!(record.code, "ABCD12");
    }

    #[test]
    fn register_rejects_invalid_code() {
        let mut directory = SessionDirectory::default();
        let err = directory
            .register(HostedSessionRegistration {
                code: "bad/code".to_string(),
                app: None,
                machine_id: "3d8e19f".to_string(),
                region: None,
                target_path: "/ws".to_string(),
            })
            .unwrap_err();

        assert_eq!(
            err,
            "Hosted session codes must be 4-32 characters using letters, numbers, - or _."
        );
    }

    #[test]
    fn register_rejects_invalid_target_path() {
        let mut directory = SessionDirectory::default();
        let err = directory
            .register(HostedSessionRegistration {
                code: "ABCD12".to_string(),
                app: None,
                machine_id: "3d8e19f".to_string(),
                region: None,
                target_path: "ws".to_string(),
            })
            .unwrap_err();

        assert_eq!(err, "Hosted session target_path must start with '/'");
    }

    #[test]
    fn public_ws_path_uses_same_origin_route_shape() {
        assert_eq!(public_ws_path("ABCD12"), Ok("/session/ABCD12/ws".to_string()));
    }

    #[test]
    fn replay_instruction_targets_machine_and_rewrites_path() {
        let record = HostedSessionRecord {
            code: "ABCD12".to_string(),
            app: Some("rts-game".to_string()),
            machine_id: "3d8e19f".to_string(),
            region: Some("ams".to_string()),
            target_path: "/ws".to_string(),
        };

        assert_eq!(
            replay_instruction(&record),
            FlyReplayInstruction {
                app: Some("rts-game".to_string()),
                instance: "3d8e19f".to_string(),
                region: Some("ams".to_string()),
                transform: Some(FlyReplayTransform {
                    path: "/ws".to_string(),
                }),
            }
        );
    }
}

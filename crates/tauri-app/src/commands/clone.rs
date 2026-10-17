use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use git_core::clone::{CloneError, CloneOptions};
use git_core::repo::RepoError;
use repo_service::clone::{DefaultCloneService, HttpsCredentialInput, ServiceCloneOutcome};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use super::emit_transfer_events;

#[derive(Default)]
pub struct CloneState(pub Arc<DefaultCloneService>);

// No `Debug` derive on the two request DTOs: they carry the HTTPS token.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpsInputDto {
    pub username: String,
    pub token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequestDto {
    pub url: String,
    pub dest: String,
    pub branch: Option<String>,
    pub recurse_submodules: bool,
    pub https: Option<HttpsInputDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneOutcomeDto {
    pub path: String,
    pub submodule_failures: Vec<String>,
    pub credential_save_failed: bool,
}

impl From<ServiceCloneOutcome> for CloneOutcomeDto {
    fn from(value: ServiceCloneOutcome) -> Self {
        Self {
            path: value.outcome.path.to_string_lossy().into_owned(),
            submodule_failures: value.outcome.submodule_failures,
            credential_save_failed: value.credential_save_failed,
        }
    }
}

/// Clone/init failures cross IPC as `{ kind, message }` so the dialog can branch on `kind`.
/// `message` is a `Display` string from git-core: it never contains the token (the token only
/// lives in `OneShotHttps`, and libgit2 error text doesn't echo credentials).
#[derive(Serialize, Debug)]
pub struct CloneErrorDto {
    pub kind: String,
    pub message: String,
}

impl From<CloneError> for CloneErrorDto {
    fn from(error: CloneError) -> Self {
        let kind = match &error {
            CloneError::InvalidUrl => "InvalidUrl".to_string(),
            CloneError::CredentialBearingUrl => "CredentialBearingUrl".to_string(),
            CloneError::DestNotEmpty => "DestNotEmpty".to_string(),
            CloneError::BranchNotFound => "BranchNotFound".to_string(),
            CloneError::Cancelled => "Cancelled".to_string(),
            CloneError::Transfer { kind, .. } => format!("{kind:?}"),
            CloneError::Io(_) => "Io".to_string(),
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}

impl From<RepoError> for CloneErrorDto {
    fn from(error: RepoError) -> Self {
        let kind = match error {
            RepoError::AlreadyRepository => "AlreadyRepository",
            _ => "InitFailed",
        };
        Self {
            kind: kind.to_string(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
pub async fn clone_repo(
    operation_id: String,
    request: CloneRequestDto,
    app: AppHandle,
    state: State<'_, CloneState>,
) -> Result<CloneOutcomeDto, CloneErrorDto> {
    let service = Arc::clone(&state.0);
    let (event_tx, event_rx) = mpsc::channel();
    emit_transfer_events(app, event_rx);
    let options = CloneOptions {
        url: request.url,
        dest: PathBuf::from(request.dest),
        branch: request.branch,
        recurse_submodules: request.recurse_submodules,
    };
    let https = request.https.map(|input| HttpsCredentialInput {
        username: input.username,
        token: input.token,
    });
    tauri::async_runtime::spawn_blocking(move || {
        service.clone_repo(&operation_id, options, https, event_tx)
    })
    .await
    .map_err(|error| CloneErrorDto {
        kind: "Io".into(),
        message: error.to_string(),
    })?
    .map(CloneOutcomeDto::from)
    .map_err(CloneErrorDto::from)
}

#[tauri::command]
pub async fn cancel_clone(
    operation_id: String,
    state: State<'_, CloneState>,
) -> Result<(), String> {
    state.0.cancel(&operation_id);
    Ok(())
}

#[tauri::command]
pub async fn init_repo(path: String) -> Result<(), CloneErrorDto> {
    git_core::repo::init_repo(std::path::Path::new(&path)).map_err(CloneErrorDto::from)
}

#[cfg(test)]
mod tests {
    use git_core::clone::{CloneError, CloneOutcome};
    use git_core::remote::TransferErrorKind;
    use repo_service::clone::ServiceCloneOutcome;

    use super::{CloneErrorDto, CloneOutcomeDto, CloneRequestDto};

    #[test]
    fn outcome_serializes_camel_case() {
        let dto = CloneOutcomeDto::from(ServiceCloneOutcome {
            outcome: CloneOutcome {
                path: "/tmp/r".into(),
                submodule_failures: vec!["deps/a".into()],
            },
            credential_save_failed: true,
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({ "path": "/tmp/r", "submoduleFailures": ["deps/a"], "credentialSaveFailed": true })
        );
    }

    #[test]
    fn errors_serialize_as_kind_and_message() {
        let cases = [
            (CloneError::InvalidUrl, "InvalidUrl"),
            (CloneError::CredentialBearingUrl, "CredentialBearingUrl"),
            (CloneError::DestNotEmpty, "DestNotEmpty"),
            (CloneError::BranchNotFound, "BranchNotFound"),
            (CloneError::Cancelled, "Cancelled"),
            (
                CloneError::Transfer {
                    kind: TransferErrorKind::MissingCredential,
                    message: "x".into(),
                },
                "MissingCredential",
            ),
        ];
        for (error, kind) in cases {
            let value = serde_json::to_value(CloneErrorDto::from(error)).unwrap();
            assert_eq!(value["kind"], kind);
            assert!(value["message"].is_string());
        }
    }

    #[test]
    fn already_repository_init_error_has_its_own_kind() {
        let dto = CloneErrorDto::from(git_core::repo::RepoError::AlreadyRepository);
        assert_eq!(
            serde_json::to_value(dto).unwrap()["kind"],
            "AlreadyRepository"
        );
    }

    #[test]
    fn request_deserializes_from_camel_case_with_null_https() {
        let request: CloneRequestDto = serde_json::from_value(serde_json::json!({
            "url": "https://example.test/r.git", "dest": "/tmp/r", "branch": null,
            "recurseSubmodules": true, "https": null
        }))
        .unwrap();
        assert!(request.recurse_submodules);
        assert!(request.https.is_none());
    }

    #[test]
    fn clone_progress_reports_its_operation_name() {
        let dto = crate::commands::TransferProgressDto::from(git_core::remote::TransferProgress {
            operation_id: "c".into(),
            operation: git_core::remote::TransferOperation::Clone,
            phase: git_core::remote::TransferPhase::Receiving,
            current: 1,
            total: 2,
            received_bytes: 3,
            message: None,
        });
        assert_eq!(dto.operation, "Clone");
    }
}

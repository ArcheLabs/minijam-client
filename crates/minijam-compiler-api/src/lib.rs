// SPDX-License-Identifier: Apache-2.0

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use tokio::{process::Command, sync::Semaphore, time::timeout};

const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_BLOB_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    C,
    Cpp,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Optimization {
    O0,
    Os,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CompileRequest {
    pub language: Language,
    pub source: String,
    pub optimization: Optimization,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainInfo {
    pub clang: String,
    pub polkavm: String,
    pub converter: String,
    pub sdk: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResponse {
    pub success: bool,
    pub blob_base64: Option<String>,
    pub code_hash: Option<String>,
    pub code_length: Option<u32>,
    pub diagnostics: Vec<String>,
    pub toolchain: ToolchainInfo,
}

#[derive(Clone, Debug)]
pub struct CompilerConfig {
    pub repository: PathBuf,
    pub image: String,
    pub docker_binary: PathBuf,
    pub direct: bool,
    pub timeout: Duration,
    pub concurrency: usize,
}

#[derive(Default)]
struct Metrics {
    total: AtomicU64,
    failed: AtomicU64,
    timed_out: AtomicU64,
}

#[derive(Clone)]
pub struct CompilerService {
    config: CompilerConfig,
    permits: Arc<Semaphore>,
    metrics: Arc<Metrics>,
}

impl CompilerService {
    pub fn new(config: CompilerConfig) -> Self {
        let concurrency = config.concurrency.max(1);
        Self {
            config,
            permits: Arc::new(Semaphore::new(concurrency)),
            metrics: Arc::new(Metrics::default()),
        }
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/internal/v1/compile", post(compile))
            .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
            .route("/readyz", get(|| async { StatusCode::NO_CONTENT }))
            .route("/health/ready", get(|| async { StatusCode::NO_CONTENT }))
            .route("/metrics", get(metrics))
            .with_state(self)
    }

    pub fn sandbox_command(
        &self,
        source: &Path,
        output: &Path,
        request: &CompileRequest,
    ) -> Command {
        if self.config.direct {
            let mut command = Command::new(self.config.repository.join("scripts/compile-service"));
            command.args([
                match request.language {
                    Language::C => "c",
                    Language::Cpp => "cpp",
                },
                source.to_str().expect("temporary source path is UTF-8"),
                output.to_str().expect("temporary output path is UTF-8"),
                match request.optimization {
                    Optimization::O0 => "O0",
                    Optimization::Os => "Os",
                },
            ]);
            command.env(
                "MINIJAM_CONVERTER_BIN",
                std::env::var("MINIJAM_CONVERTER_BIN")
                    .unwrap_or_else(|_| "/usr/local/bin/polkavm-to-jam".into()),
            );
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            command.kill_on_drop(true);
            return command;
        }
        let mut command = Command::new(&self.config.docker_binary);
        command.args([
            "run",
            "--rm",
            "--network=none",
            "--read-only",
            "--user=65532:65532",
            "--cpus=1",
            "--memory=512m",
            "--pids-limit=64",
            "--security-opt=no-new-privileges",
            "--cap-drop=ALL",
            "--env",
            "MINIJAM_CONVERTER_BIN=/usr/local/bin/polkavm-to-jam",
        ]);
        command.arg("--mount").arg(format!(
            "type=bind,src={},dst=/workspace,readonly",
            self.config.repository.display()
        ));
        command.arg("--mount").arg(format!(
            "type=bind,src={},dst=/input/service.{},readonly",
            source.display(),
            match request.language {
                Language::C => "c",
                Language::Cpp => "cpp",
            }
        ));
        command
            .arg("--mount")
            .arg(format!("type=bind,src={},dst=/output", output.display()));
        command.args([
            "--tmpfs",
            "/tmp:rw,noexec,nosuid,size=64m",
            &self.config.image,
        ]);
        command.args([
            "/workspace/scripts/compile-service",
            match request.language {
                Language::C => "c",
                Language::Cpp => "cpp",
            },
            match request.language {
                Language::C => "/input/service.c",
                Language::Cpp => "/input/service.cpp",
            },
            "/output",
            match request.optimization {
                Optimization::O0 => "O0",
                Optimization::Os => "Os",
            },
        ]);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.kill_on_drop(true);
        command
    }

    async fn execute(&self, request: CompileRequest) -> CompileResponse {
        self.metrics.total.fetch_add(1, Ordering::Relaxed);
        if request.source.len() > MAX_SOURCE_BYTES {
            return self.failure("source exceeds 256 KiB");
        }
        let _permit = self
            .permits
            .acquire()
            .await
            .expect("compiler semaphore closed");
        let temp = match tempfile::tempdir() {
            Ok(temp) => temp,
            Err(error) => return self.failure(&error.to_string()),
        };
        let extension = if request.language == Language::C {
            "c"
        } else {
            "cpp"
        };
        let source = temp.path().join(format!("service.{extension}"));
        let output = temp.path().join("output");
        if let Err(error) = std::fs::write(&source, request.source.as_bytes())
            .and_then(|_| std::fs::create_dir(&output))
        {
            return self.failure(&error.to_string());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(error) =
                std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755))
                    .and_then(|_| {
                        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o777))
                    })
            {
                return self.failure(&error.to_string());
            }
        }
        let result = timeout(
            self.config.timeout,
            self.sandbox_command(&source, &output, &request).output(),
        )
        .await;
        let output_result = match result {
            Err(_) => {
                self.metrics.timed_out.fetch_add(1, Ordering::Relaxed);
                return self.failure("compilation timed out");
            }
            Ok(Err(error)) => return self.failure(&error.to_string()),
            Ok(Ok(output_result)) => output_result,
        };
        if !output_result.status.success() {
            let diagnostics = String::from_utf8_lossy(&output_result.stderr);
            return self.failure(&diagnostics.chars().take(16_384).collect::<String>());
        }
        let blob = match std::fs::read(output.join("service.blob")) {
            Ok(blob) if blob.len() <= MAX_BLOB_BYTES => blob,
            Ok(_) => return self.failure("compiler output exceeds 4 MiB"),
            Err(error) => return self.failure(&error.to_string()),
        };
        let hash = minijam_protocol::blake2_256(&blob);
        CompileResponse {
            success: true,
            blob_base64: Some(STANDARD.encode(&blob)),
            code_hash: Some(hex(&hash)),
            code_length: Some(blob.len() as u32),
            diagnostics: Vec::new(),
            toolchain: toolchain(),
        }
    }

    fn failure(&self, diagnostic: &str) -> CompileResponse {
        self.metrics.failed.fetch_add(1, Ordering::Relaxed);
        CompileResponse {
            success: false,
            blob_base64: None,
            code_hash: None,
            code_length: None,
            diagnostics: vec![diagnostic.to_owned()],
            toolchain: toolchain(),
        }
    }
}

async fn compile(
    State(service): State<CompilerService>,
    Json(request): Json<CompileRequest>,
) -> Json<CompileResponse> {
    Json(service.execute(request).await)
}

async fn metrics(State(service): State<CompilerService>) -> String {
    format!(
        "minijam_compiler_requests_total {}\nminijam_compiler_failures_total {}\nminijam_compiler_timeouts_total {}\n",
        service.metrics.total.load(Ordering::Relaxed),
        service.metrics.failed.load(Ordering::Relaxed),
        service.metrics.timed_out.load(Ordering::Relaxed),
    )
}

fn toolchain() -> ToolchainInfo {
    ToolchainInfo {
        clang: "20".into(),
        polkavm: "0.30.0".into(),
        converter: "jam-program-blob-common-0.1.28".into(),
        sdk: "minijam-sdk-v1".into(),
    }
}

fn hex(bytes: &[u8]) -> String {
    format!(
        "0x{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> CompilerService {
        CompilerService::new(CompilerConfig {
            repository: PathBuf::from("/repo"),
            image: "minijam-compiler:test".into(),
            docker_binary: PathBuf::from("docker"),
            direct: false,
            timeout: Duration::from_secs(10),
            concurrency: 2,
        })
    }

    #[test]
    fn sandbox_has_mandatory_isolation_and_fixed_flags() {
        let request = CompileRequest {
            language: Language::Cpp,
            source: "int x;".into(),
            optimization: Optimization::Os,
        };
        let command =
            service().sandbox_command(Path::new("/tmp/in.cpp"), Path::new("/tmp/out"), &request);
        let args = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        for required in [
            "--network=none",
            "--read-only",
            "--user=65532:65532",
            "--memory=512m",
            "--pids-limit=64",
            "--cap-drop=ALL",
        ] {
            assert!(args.iter().any(|arg| arg == required), "missing {required}");
        }
        assert!(args.iter().any(|arg| arg == "cpp"));
        assert!(args.iter().any(|arg| arg == "Os"));
    }

    #[test]
    fn direct_backend_runs_repository_compiler_without_nested_docker() {
        let mut service = service();
        service.config.direct = true;
        let request = CompileRequest {
            language: Language::C,
            source: "int x;".into(),
            optimization: Optimization::O0,
        };
        let command =
            service.sandbox_command(Path::new("/tmp/in.c"), Path::new("/tmp/out"), &request);
        assert_eq!(
            command.as_std().get_program(),
            Path::new("/repo/scripts/compile-service")
        );
        let args = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert_eq!(args, ["c", "/tmp/in.c", "/tmp/out", "O0"]);
    }

    #[tokio::test]
    async fn oversized_source_is_rejected_before_spawning_compiler() {
        let response = service()
            .execute(CompileRequest {
                language: Language::C,
                source: "x".repeat(MAX_SOURCE_BYTES + 1),
                optimization: Optimization::O0,
            })
            .await;
        assert!(!response.success);
        assert!(response.diagnostics[0].contains("256 KiB"));
    }

    fn fake_service(scenario: &str, timeout: Duration) -> CompilerService {
        CompilerService::new(CompilerConfig {
            repository: PathBuf::from("/repo"),
            image: format!("minijam-compiler:test-{scenario}"),
            docker_binary: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/fake-docker"),
            direct: false,
            timeout,
            concurrency: 2,
        })
    }

    fn request() -> CompileRequest {
        CompileRequest {
            language: Language::C,
            source: "int minijam_refine(void) { return 0; }".into(),
            optimization: Optimization::Os,
        }
    }

    #[tokio::test]
    async fn identical_compiles_return_identical_blob_and_hash() {
        let service = fake_service("success", Duration::from_secs(1));
        let first = service.execute(request()).await;
        let second = service.execute(request()).await;

        assert!(first.success && second.success);
        assert_eq!(first.blob_base64, second.blob_base64);
        assert_eq!(first.code_hash, second.code_hash);
    }

    #[tokio::test]
    async fn compiler_diagnostics_are_bounded_and_returned() {
        let service = fake_service("diagnostics", Duration::from_secs(1));
        let response = service.execute(request()).await;

        assert!(!response.success);
        assert_eq!(response.diagnostics, vec!["syntax error"]);
    }

    #[tokio::test]
    async fn compiler_process_is_killed_at_timeout() {
        let service = fake_service("timeout", Duration::from_millis(20));
        let response = service.execute(request()).await;

        assert!(!response.success);
        assert!(response.diagnostics[0].contains("timed out"));
    }

    #[tokio::test]
    async fn oversized_compiler_output_is_rejected() {
        let service = fake_service("oversized", Duration::from_secs(2));
        let response = service.execute(request()).await;

        assert!(!response.success);
        assert!(
            response.diagnostics[0].contains("4 MiB"),
            "{:?}",
            response.diagnostics
        );
    }
}

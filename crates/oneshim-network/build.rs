//! Proto 코드 생성 빌드 스크립트
//!
//! grpc feature 활성화 시 tonic-build로 Rust 코드를 생성합니다.

fn main() {
    // grpc feature가 활성화된 경우에만 proto 컴파일
    if std::env::var("CARGO_FEATURE_GRPC").is_ok() {
        compile_protos();
    }
}

fn compile_protos() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let proto_root = std::path::Path::new(&manifest_dir).join("../../../api/proto");

    // Proto 파일들
    let protos: Vec<std::path::PathBuf> = vec![
        proto_root.join("oneshim/v1/common/types.proto"),
        proto_root.join("oneshim/v1/common/enums.proto"),
        proto_root.join("oneshim/v1/common/errors.proto"),
        proto_root.join("oneshim/v1/auth/auth.proto"),
        proto_root.join("oneshim/v1/auth/session.proto"),
        proto_root.join("oneshim/v1/auth/device.proto"),
        proto_root.join("oneshim/v1/user_context/events.proto"),
        proto_root.join("oneshim/v1/user_context/frames.proto"),
        proto_root.join("oneshim/v1/user_context/suggestions.proto"),
        proto_root.join("oneshim/v1/user_context/batch.proto"),
        proto_root.join("oneshim/v1/user_context/service.proto"),
        proto_root.join("oneshim/v1/monitoring/metrics.proto"),
    ];

    // Proto 파일 존재 확인
    let mut missing_protos = false;
    for proto in &protos {
        if !proto.exists() {
            eprintln!("Warning: Proto file not found: {:?}", proto);
            missing_protos = true;
        }
    }

    if missing_protos {
        eprintln!("Some proto files are missing. Skipping code generation.");
        return;
    }

    // tonic-build로 코드 생성
    let out_dir = std::path::Path::new(&manifest_dir).join("src/proto/generated");
    std::fs::create_dir_all(&out_dir).expect("Failed to create output directory");

    tonic_build::configure()
        .build_server(false) // 클라이언트만 생성
        .build_client(true)
        .out_dir(&out_dir)
        .compile_protos(&protos, &[&proto_root])
        .expect("Failed to compile protos");

    // 재빌드 트리거
    println!("cargo:rerun-if-changed={}", proto_root.display());
}
